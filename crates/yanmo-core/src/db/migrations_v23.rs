//! 迁移 v23：**模块管理页的偏好**——收藏 / 置顶两列。
//!
//! ⚠️ 与 v1~v22 同样**一旦发布不可再改**。
//!
//! 两列都是**库级偏好**（跟书无关、不进导出），只影响管理页的排序与收纳。
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，先查 `PRAGMA table_info` 再决定
//! （迁移铁律第 3 条）。

use rusqlite::Connection;

use crate::error::Result;

/// v23 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

/// 要加哪些列，**先查再决定**——重跑不会撞 "duplicate column name"。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "modules", "favorite")? {
        steps.push(
            "ALTER TABLE modules ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0".to_string(),
        );
    }
    if !column_exists(conn, "modules", "pinned")? {
        steps.push(
            "ALTER TABLE modules ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0".to_string(),
        );
    }
    Ok(steps)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // 表名是常量字面量，不是外部输入——PRAGMA 不支持参数占位，只能拼
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
