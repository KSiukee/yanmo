//! 迁移 v28：**外部文本导入的批次账**。
//!
//! 一批导入是一笔账：来源（备忘录 / 文件 / 粘贴）、切成几条、什么时候导的、撤没撤。
//! 撤销只动这一批碎片，**绝不碰正文**。
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，先查 `PRAGMA table_info` 再决定（照 v12）。

use rusqlite::Connection;

use crate::error::Result;

/// 建表 + 建索引（幂等，可以静态跑）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS import_batches (
        id         TEXT    PRIMARY KEY,
        work_id    INTEGER NOT NULL,
        kind       TEXT    NOT NULL,
        label      TEXT    NOT NULL,
        source     TEXT    NOT NULL,
        count      INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        undone_at  INTEGER
    )",
    "CREATE INDEX IF NOT EXISTS idx_import_batches_work
        ON import_batches(work_id, created_at DESC)",
];

/// 碎片表上的批次号：新列先查再决定，重跑不撞车。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "fragments", "import_batch")? {
        steps.push("ALTER TABLE fragments ADD COLUMN import_batch TEXT".to_string());
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
