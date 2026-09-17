//! 磁盘即 `.md`：强镜像的**账本与渲染**——库这一侧，**不碰文件系统**。
//!
//! # 这个模块管什么
//!
//! - 渲染：这本书该有哪几个 `.md`、每个文件里是什么字节（[`Store::render_mirror`]）；
//! - 便宜对账：库这边变了没有（[`Store::mirror_pending`]）——**只读元数据与指纹，不读正文**；
//! - 账本：上次写进镜像的落点与两把指纹（[`Store::mirror_state`] / [`Store::mirror_record`]）；
//! - 开关（[`Store::mirror_enabled`]）。
//!
//! 决定"哪些要写 / 改名 / 删、哪些被人改过不能动"是 [`super::mirror_plan`] 的事；
//! 真正落盘在壳里（`yanmo_app::mirror`）——它要异步、要把连打合并成一次、要跟着应用生命周期收尾。
//!
//! # 五条硬要求
//!
//! ① 落盘后异步写（节拍在壳里）；② 目录结构稳定可预测（`<书名>-<id>/<卷>/<章>.md`，
//! 与导出同一套命名，见 [`super::tree_path`]）；③ 改名 / 移动同步 rename、不留孤儿；
//! ④ 外部改过的一律**只看不动**、绝不静默丢弃；⑤ 幂等 + 稳定排序。
//!
//! # 账本为什么在库里（而不是数据目录旁挂一个清单）
//!
//! 它必须与正文**同库同事务**：备份、恢复、换位置之后账才跟得上；旁挂清单一旦与库脱节，
//! 启动时会把满盘文件都误报成"被外面改过"。表见 `db/migrations_v15`。
//!
//! # 标题那一行
//!
//! 每个文件以 `# 显示章名` 开头：单拎一份出来也知道这是哪一章，记事本里一眼能读。
//! 显示名是**渲染后**的那一份（号已按位置算好），与目录树、导出看到的一致。

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};

use super::mirror_plan::{MirrorEntry, MirrorFile, MirrorRecord};
use super::{tree_path, Store};
use crate::error::Result;
use crate::text::content_hash;
use crate::time::now_millis;

impl Store {
    /// 镜像开关。**默认开**——"稿子永远是记事本能打开的 `.md`"是硬承诺，不是可选项；
    /// 关掉是作者的自由，但关之前他不会因为不知道而丢了这个能力。
    pub fn mirror_enabled(&self) -> Result<bool> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = 'mirror.enabled'", [], |r| r.get(0))
            .optional()?;
        Ok(value.as_deref() != Some("0"))
    }

    /// 写开关。**只写这一格，不动任何文件**：关掉镜像不会删掉作者磁盘上已有的 `.md`
    /// （那可能是他唯一带走的一份），重新打开时按需补写。
    pub fn set_mirror_enabled(&mut self, on: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES('mirror.enabled', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![if on { "1" } else { "0" }, now_millis()],
        )?;
        Ok(())
    }

    /// 要照看的书：活着的 + **账还留着但书已经不在了的**（后者要按账把文件收掉）。
    pub fn mirror_works(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM works WHERE deleted_at IS NULL
             UNION
             SELECT work_id FROM mirror_state
             ORDER BY 1",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 这本书该有哪些镜像文件（含正文）。
    ///
    /// 已删 / 不存在的书返回空——调用方按账把旧文件收掉。
    pub fn render_mirror(&self, work_id: i64) -> Result<Vec<MirrorFile>> {
        let Some(title) = self.live_work_title(work_id)? else {
            return Ok(Vec::new());
        };
        let nodes = self.list_nodes(work_id)?;
        let hashes = self.body_hashes(work_id)?;
        let root = tree_path::work_folder_name(work_id, &title);
        let mut out = Vec::new();
        tree_path::walk(&nodes, |node, here| {
            if !node.kind.holds_body() {
                return Ok(());
            }
            let body = self.read_body(node.id)?;
            let text = tree_path::normalize(&format!("# {}\n\n{body}", heading(node)));
            out.push(MirrorFile {
                node_id: node.id,
                relative_path: format!("{root}/{here}.md"),
                body_hash: hashes.get(&node.id).cloned().unwrap_or_default(),
                file_hash: content_hash(&text),
                content: text.into_bytes(),
            });
            Ok(())
        })?;
        Ok(out)
    }

    /// 这本书上一次写进镜像的账。
    pub fn mirror_state(&self, work_id: i64) -> Result<Vec<MirrorEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT node_id, relative_path, body_hash, file_hash, size_bytes, conflict
               FROM mirror_state WHERE work_id = ?1 ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![work_id], |r| {
            Ok(MirrorEntry {
                node_id: r.get(0)?,
                relative_path: r.get(1)?,
                body_hash: r.get(2)?,
                file_hash: r.get(3)?,
                size_bytes: r.get(4)?,
                conflict: r.get::<_, i64>(5)? != 0,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 库这边变了没有——**只读元数据与指纹，不读正文**（定期对账靠它跑得勤）。
    ///
    /// 已登记的冲突**不算"没写完"**：那些文件我们故意不动，若算进去就会每两秒把整本书的
    /// 正文重扫一遍。作者的定夺是下一步的事。
    pub fn mirror_pending(&self, work_id: i64) -> Result<bool> {
        let scan = self.mirror_scan(work_id)?;
        let state = self.mirror_state(work_id)?;
        let by_node: HashMap<i64, &MirrorEntry> = state.iter().map(|e| (e.node_id, e)).collect();
        for entry in state.iter().filter(|e| !e.conflict) {
            match scan.get(&entry.node_id) {
                Some((path, hash)) if path == &entry.relative_path && hash == &entry.body_hash => {}
                _ => return Ok(true),
            }
        }
        for (node_id, (path, hash)) in &scan {
            match by_node.get(node_id) {
                Some(entry) if &entry.relative_path == path && &entry.body_hash == hash => {}
                _ => return Ok(true),
            }
        }
        Ok(false)
    }

    /// 把这次对账的结果记进账本（**执行完动作之后**调）。
    ///
    /// 整本书一次替换：账只反映"最后一次落定的镜像"，半截状态没有意义。
    /// （没有单独传"要删的账"——整本替换之后，那些行自然就不在了。）
    pub fn mirror_record(&mut self, work_id: i64, records: &[MirrorRecord]) -> Result<()> {
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM mirror_state WHERE work_id = ?1", params![work_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO mirror_state
                    (node_id, work_id, relative_path, body_hash, file_hash, size_bytes, written_at, conflict)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for record in records {
                stmt.execute(params![
                    record.node_id,
                    work_id,
                    record.relative_path,
                    record.body_hash,
                    record.file_hash,
                    record.size_bytes,
                    now,
                    i64::from(record.conflict),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 停用镜像时把账清掉（**只清账，不删文件**）：作者磁盘上那些 `.md` 是他自己的东西。
    pub fn mirror_forget(&mut self, work_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM mirror_state WHERE work_id = ?1", params![work_id])?;
        Ok(())
    }

    /// 活着的作品标题（已删 / 不存在给 `None`）。
    fn live_work_title(&self, work_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT title FROM works WHERE id = ?1 AND deleted_at IS NULL",
                params![work_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// 这本书每个节点的正文指纹（不读正文）。
    fn body_hashes(&self, work_id: i64) -> Result<HashMap<i64, String>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.node_id, c.content_hash FROM node_contents c
               JOIN nodes n ON n.id = c.node_id
              WHERE n.work_id = ?1",
        )?;
        let rows = stmt.query_map(params![work_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (node_id, hash) = row?;
            out.insert(node_id, hash);
        }
        Ok(out)
    }

    /// 「该有哪些文件」的廉价版本：只要路径与正文指纹，**不读正文**。
    fn mirror_scan(&self, work_id: i64) -> Result<HashMap<i64, (String, String)>> {
        let Some(title) = self.live_work_title(work_id)? else {
            return Ok(HashMap::new());
        };
        let nodes = self.list_nodes(work_id)?;
        let hashes = self.body_hashes(work_id)?;
        let root = tree_path::work_folder_name(work_id, &title);
        let mut out = HashMap::new();
        tree_path::walk(&nodes, |node, here| {
            if node.kind.holds_body() {
                out.insert(
                    node.id,
                    (
                        format!("{root}/{here}.md"),
                        hashes.get(&node.id).cloned().unwrap_or_default(),
                    ),
                );
            }
            Ok(())
        })?;
        Ok(out)
    }
}

/// 文件第一行的标题：显示名（号已按位置渲染好）；没起名的退到结构标识，
/// 与文件名同一套兜底——**空标题绝不能变成一行空的 `# `**。
fn heading(node: &super::NodeSummary) -> String {
    if node.title_rendered.trim().is_empty() {
        node.kind.as_str().to_string()
    } else {
        node.title_rendered.clone()
    }
}
