//! 全文检索：FTS5 + trigram 分词。
//!
//! - **≥3 字**：走 FTS5 `MATCH`（trigram 索引，中文按子串命中，不依赖分词）；
//! - **1–2 字**：trigram 在结构上就表示不了这么短的串，回退 `instr` 全扫——
//!   慢一点但**结果是对的**（中文里两个字太常见，"大纲""伏笔"不能搜不到）；
//! - **高亮**：不用 FTS5 的 highlight（短查询路径没有 MATCH），统一在 Rust 侧算片段，
//!   两条路径行为一致；
//! - **已删除的节点与作品一律不出现在结果里**（软删除也要从搜索结果消失）。
//!
//! 查询语义是**短语**：`"反派 动机"` 要求这段文字连续出现（含空格），
//! 不是"两个词都出现"。给作者的检索里，"我写过这句话"比"两个词都出现过"更常用。

use rusqlite::params;
use serde_json::json;

use super::Store;
use crate::error::Result;
use crate::text;

/// 高亮片段两侧各取多少字。
const SNIPPET_CONTEXT: usize = 24;
/// 不指定时的结果上限。
const DEFAULT_LIMIT: usize = 50;
/// 单次检索的硬上限——防一次检索把整库拖回来。
const MAX_LIMIT: usize = 500;
/// trigram 能表示的最小长度。
const MIN_MATCH_CHARS: usize = 3;

/// 一条命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub node_id: i64,
    pub work_id: i64,
    /// 节点标题（章名 / 篇名）
    pub title: String,
    /// 命中周围的片段；命中只在标题里时为空串
    pub snippet: String,
    /// 标题本身是否命中（界面可以据此高亮标题）
    pub matched_title: bool,
}

/// 公共部分：只保留未删除的节点与作品。
const BASE_SQL: &str = "SELECT node_fts.rowid, n.work_id, node_fts.title, node_fts.body
     FROM node_fts
     JOIN nodes n ON n.id = node_fts.rowid
     JOIN works w ON w.id = n.work_id
     WHERE n.deleted_at IS NULL AND w.deleted_at IS NULL
       AND (?1 IS NULL OR n.work_id = ?1)";

impl Store {
    /// 在（可选的）某部作品里检索 `query`。
    pub fn search(&self, query: &str, work_id: Option<i64>, limit: usize) -> Result<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = match limit {
            0 => DEFAULT_LIMIT,
            n => n.min(MAX_LIMIT),
        } as i64;
        let long_enough = query.chars().count() >= MIN_MATCH_CHARS;

        let sql = if long_enough {
            format!("{BASE_SQL} AND node_fts MATCH ?2 ORDER BY n.updated_at DESC LIMIT ?3")
        } else {
            // lower() 让短查询也大小写不敏感（与 FTS 的 ASCII 折叠口径一致）
            format!(
                "{BASE_SQL} AND (instr(lower(node_fts.title), lower(?2)) > 0
                     OR instr(lower(node_fts.body), lower(?2)) > 0)
                 ORDER BY n.updated_at DESC LIMIT ?3"
            )
        };
        // MATCH 走 FTS5 查询语法：整体加双引号（内部引号翻倍），避免用户输入被当成语法
        let needle = if long_enough {
            format!("\"{}\"", query.replace('"', "\"\""))
        } else {
            query.to_string()
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id, needle, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;

        let mut hits = Vec::new();
        for row in rows {
            let (node_id, work_id, title, body) = row?;
            hits.push(SearchHit {
                node_id,
                work_id,
                matched_title: text::find_char_index(&title, query).is_some(),
                snippet: text::snippet(&body, query, SNIPPET_CONTEXT).unwrap_or_default(),
                title,
            });
        }
        Ok(hits)
    }

    /// 重建检索索引（导入、修复、或怀疑索引与正文不一致时用）。返回重建后的条目数。
    ///
    /// 平时不靠它——索引由触发器跟着写入走；这是**兜底与自证**的入口。
    pub fn reindex(&mut self) -> Result<usize> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM node_fts", [])?;
        tx.execute(
            "INSERT INTO node_fts(rowid, title, body)
             SELECT n.id, n.title, COALESCE(c.body, '')
             FROM nodes n LEFT JOIN node_contents c ON c.node_id = n.id",
            [],
        )?;
        // 数一数 + 留痕都在这个事务里（评审：中等 6）：报失败必须意味着"索引真的没重建"
        let rows: i64 = tx.query_row("SELECT COUNT(*) FROM node_fts", [], |r| r.get(0))?;
        Self::record_in(&self.device_id, &tx, "node_fts", 0, "reindex", json!({ "rows": rows }))?;
        tx.commit()?;
        Ok(rows as usize)
    }
}
