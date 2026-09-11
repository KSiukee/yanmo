//! 作品（`works`）读写。**书架是一等公民**：多作品是默认形态，不是附加功能。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{Error, Result};
use crate::model::{NodeKind, Work, WorkKind};
use crate::time::now_millis;

/// 作品字段列表（顺序与 [`WorkRow`] 对应）。
const COLS: &str = "id, kind, title, target_words, created_at, updated_at, opened_at";

/// 一行的原始取值——先取成朴素类型，再**在 Rust 侧校验**（不在 SQL 里猜着读）。
type WorkRow = (i64, String, String, Option<i64>, i64, i64, Option<i64>);

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn build(row: WorkRow) -> Result<Work> {
    Ok(Work {
        id: row.0,
        kind: WorkKind::parse(&row.1)?,
        title: row.2,
        target_words: row.3,
        created_at: row.4,
        updated_at: row.5,
        opened_at: row.6,
    })
}

/// 新建作品的根节点模板：**只是给个起点，不是结构约束**（深度不写死）。
///
/// 长篇给一个空卷，方便往里加章；文章与短篇集直接给"单篇"——**根节点即正文，零层级**。
fn root_template(kind: WorkKind, work_title: &str) -> (NodeKind, String) {
    match kind {
        WorkKind::Novel => (NodeKind::Volume, "第一卷".to_string()),
        WorkKind::Article | WorkKind::Collection => (NodeKind::Piece, work_title.to_string()),
    }
}

impl Store {
    /// 新建作品：**同一个事务里连根节点一起建**——失败不留半个作品。
    pub fn create_work(&mut self, kind: WorkKind, title: &str) -> Result<Work> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("作品标题不能为空".to_string()));
        }
        let now = now_millis();
        let (root_kind, root_title) = root_template(kind, title);

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO works(kind, title, created_at, updated_at, opened_at)
             VALUES(?1, ?2, ?3, ?3, ?3)",
            params![kind.as_str(), title, now],
        )?;
        let work_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO nodes(work_id, parent_id, node_kind, title, sort_order, created_at, updated_at)
             VALUES(?1, NULL, ?2, ?3, 0, ?4, ?4)",
            params![work_id, root_kind.as_str(), root_title, now],
        )?;
        let root_id = tx.last_insert_rowid();
        tx.commit()?;

        self.record("works", work_id, "create", json!({ "kind": kind.as_str(), "title": title }))?;
        self.record("nodes", root_id, "create", json!({ "work_id": work_id, "root": true }))?;

        Ok(Work {
            id: work_id,
            kind,
            title: title.to_string(),
            target_words: None,
            created_at: now,
            updated_at: now,
            opened_at: Some(now),
        })
    }

    /// 书架列表：**未删除**的作品，最近打开优先，其次最近编辑。
    pub fn list_works(&self) -> Result<Vec<Work>> {
        let sql = format!(
            "SELECT {COLS} FROM works WHERE deleted_at IS NULL
             ORDER BY opened_at IS NULL, opened_at DESC, updated_at DESC, id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], read_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(build(row?)?);
        }
        Ok(out)
    }

    /// 取单个作品（含已删除的也能取到——回收站与恢复要用）。
    pub fn get_work(&self, id: i64) -> Result<Work> {
        let sql = format!("SELECT {COLS} FROM works WHERE id = ?1");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_row)
            .optional()?
            .ok_or_else(|| Error::Invalid(format!("作品不存在：{id}")))?;
        build(raw)
    }

    /// 改名。
    pub fn rename_work(&mut self, id: i64, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("作品标题不能为空".to_string()));
        }
        let affected = self.conn.execute(
            "UPDATE works SET title = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![title, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::Invalid(format!("作品不存在或已删除：{id}")));
        }
        self.record("works", id, "rename", json!({ "title": title }))
    }

    /// 记一次"打开"——书架排序只看它，不碰编辑时间。
    pub fn touch_work_opened(&mut self, id: i64) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE works SET opened_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::Invalid(format!("作品不存在或已删除：{id}")));
        }
        Ok(())
    }

    /// 软删除：只打时间戳，正文与历史都留着（回收站与误删撤销靠它）。
    pub fn soft_delete_work(&mut self, id: i64) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE works SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::Invalid(format!("作品不存在或已删除：{id}")));
        }
        self.record("works", id, "delete", json!({}))
    }
}

/// 供同层其他模块复用的作品存在性检查（防"往已删除的作品里写东西"）。
pub(super) fn ensure_alive(conn: &Connection, work_id: i64) -> Result<()> {
    let alive: Option<i64> = conn
        .query_row(
            "SELECT id FROM works WHERE id = ?1 AND deleted_at IS NULL",
            params![work_id],
            |r| r.get(0),
        )
        .optional()?;
    match alive {
        Some(_) => Ok(()),
        None => Err(Error::Invalid(format!("作品不存在或已删除：{work_id}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_template_is_zero_level_piece() {
        let (kind, title) = root_template(WorkKind::Article, "我的第一篇");
        assert_eq!(kind, NodeKind::Piece);
        assert_eq!(title, "我的第一篇", "单篇的根节点应当就是这篇文章本身");
    }

    #[test]
    fn novel_template_starts_with_a_volume() {
        let (kind, title) = root_template(WorkKind::Novel, "长夜");
        assert_eq!(kind, NodeKind::Volume);
        assert_eq!(title, "第一卷");
    }
}
