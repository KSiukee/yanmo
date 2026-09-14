//! **把整本书的编号写法统一到当前这一档**：`第{$N}章` ⇄ `第{$N_ZH}章` ⇄ `第{$N:3}章`。
//!
//! # 为什么是"先说清再动手"
//!
//! 设置里的命名规则只影响**以后新建**的条目（不动已有名字是铁律）。作者写了三十章之后改了主意
//! （想把阿拉伯数字换成中文数字），需要一个**显式动作**把已有的换过去——所以这里永远是两步：
//! [`Store::preview_naming_rewrite`] 先列出"哪几章会变成什么"，作者确认后再
//! [`Store::apply_naming_rewrite`]。绝不静默改稿。
//!
//! # 动什么、不动什么
//!
//! - **只换编号写法**：`第{$N}章 灯` → `第{$N_ZH}章 灯`（章名一个字不动）；
//! - 手写的阿拉伯数字（`第12章 灯`）→ 换成宏（位置为准，旧数字丢掉）；
//! - **没有编号的一个都不动**：`序章` / `楔子` / `番外` / 散文集里自起的名字；
//! - 目标是「不编号」时返回空清单：那是"以后新建的"，抹掉已有章的名字不是这个动作该干的事。
//!
//! 每次改名都走 `node_edit::rename_node_in`（或 [`Store::rename_node`]）——**留 op-log、同步检索索引**，
//! 与手动改名同一条路；整批改写在**一个事务**里完成（失败一条都不留，见 `apply_naming_rewrite`）。

use rusqlite::params;
use serde_json::json;

use super::node_edit;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{NamingStyle, NodeKind};
use crate::numbering;

/// 一处将要发生的改动：`before` → `after`（界面照着它摆预览）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NamingRewrite {
    pub node_id: i64,
    pub kind: String,
    pub before: String,
    pub after: String,
}

impl Store {
    /// 按**当前落定的命名规则**算出"整本书哪些条目会变、变成什么"。**只算不改**。
    pub fn preview_naming_rewrite(&self, work_id: i64) -> Result<Vec<NamingRewrite>> {
        super::work::ensure_alive(&self.conn, work_id)?;
        let style = self.naming_style(work_id)?;
        let mut out = Vec::new();
        for node in self.list_nodes(work_id)? {
            if let Some(after) = rewritten_title(&node.title, node.kind, style) {
                out.push(NamingRewrite {
                    node_id: node.id,
                    kind: node.kind.as_str().to_string(),
                    before: node.title,
                    after,
                });
            }
        }
        Ok(out)
    }

    /// 执行上面那份清单（**照预览里的 `after` 改**，不重新算一遍——作者看的就是它）。
    ///
    /// 返回改了几条。逐条走 `rename_node`：留痕、同步索引、失败立刻报错（不半途吞掉）。
    pub fn apply_naming_rewrite(&mut self, work_id: i64, rewrites: &[NamingRewrite]) -> Result<usize> {
        super::work::ensure_alive(&self.conn, work_id)?;
        // **一个事务改完**：这个动作可能改掉一整本书的标题。逐条各自提交的话，中途任何一步失败
        // （节点刚被删、深度上限、磁盘错误、进程被杀）都会留下"半本中文数字、半本阿拉伯数字"，
        // 而作者拿不到撤销（2026-09-15 代码质量评审：严重 4）。
        // 同一份代码里 `create_work` / `write_body_counted` 就是这么写的：要么全成，要么全不成。
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for rewrite in rewrites {
            // 改之前确认它还在、还归这本书（作者可能刚删了或换了书）
            let owner: Option<i64> = tx
                .query_row(
                    "SELECT work_id FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                    params![rewrite.node_id],
                    |r| r.get(0),
                )
                .ok();
            match owner {
                Some(id) if id == work_id => {}
                _ => {
                    // 直接 return：事务没提交，前面改过的那些**一条都不会留下**
                    return Err(Error::invalid_with(
                        codes::NODE_GONE,
                        [("node_id", rewrite.node_id.to_string())],
                    ))
                }
            }
            node_edit::rename_node_in(&tx, rewrite.node_id, &rewrite.after)?;
            changed += 1;
        }
        tx.commit()?;
        // 留痕放在提交之后（与 `create_work` 同一口径）：留痕失败不该把"已经成功"的操作报成失败
        for rewrite in rewrites {
            self.record(
                "nodes",
                rewrite.node_id,
                "rename",
                json!({ "title": rewrite.after.trim() }),
            )?;
        }
        Ok(changed)
    }
}

/// 一条标题要不要改、改成什么（`None` = 一个字都不动）。
fn rewritten_title(title: &str, kind: NodeKind, style: NamingStyle) -> Option<String> {
    if let Some(macroed) = numbering::rewrite_counter(title, style) {
        return Some(macroed);
    }
    let (prefix, suffix) = node_edit::naming_words(kind);
    numbering::rewrite_literal(title, prefix, suffix, style)
}
