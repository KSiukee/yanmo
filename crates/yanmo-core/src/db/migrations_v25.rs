//! 迁移 v25：**认证锚定到载荷**——认证版本与日期两列。
//!
//! ⚠️ 与 v1~v24 同样**一旦发布不可再改**。
//!
//! `certified` 这一列 v22 就有；这一版补上"哪一版认证"和"哪天认证"。
//! 认证的真源仍是**核心验签后的清单**，不是模块自述；这两列只是把验签结果落账。
//! `0` = 没认证（老库默认）。

use rusqlite::Connection;

use crate::error::Result;

/// v25 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "modules", "cert_edition")? {
        steps.push(
            "ALTER TABLE modules ADD COLUMN cert_edition INTEGER NOT NULL DEFAULT 0".to_string(),
        );
    }
    if !column_exists(conn, "modules", "certified_at")? {
        steps.push(
            "ALTER TABLE modules ADD COLUMN certified_at INTEGER NOT NULL DEFAULT 0".to_string(),
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
