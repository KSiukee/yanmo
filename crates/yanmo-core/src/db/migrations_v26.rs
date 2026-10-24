//! 迁移 v26：模块的**后台运行**偏好。
//!
//! ⚠️ 与 v1~v25 同样**一旦发布不可再改**。
//!
//! 语义：用户在管理页离开时，这个模块是否允许继续跑（默认否——离开就停）。
//! 它**不是自启开关**：模块仍然只能由用户点「启动」拉起；没有任何开机/定时/私启路径。

use rusqlite::Connection;

use crate::error::Result;

pub const STEPS: &[&str] = &[];

pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "modules", "background")? {
        steps.push(
            "ALTER TABLE modules ADD COLUMN background INTEGER NOT NULL DEFAULT 0".to_string(),
        );
    }
    Ok(steps)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
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
