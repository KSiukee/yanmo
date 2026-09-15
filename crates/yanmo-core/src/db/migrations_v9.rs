//! 迁移 v9：叩问**选题与偏好**要的三样补丁。
//!
//! ⚠️ 与 v1~v8 同样**一旦发布不可再改**。
//!
//! # 补了哪三样
//!
//! 1. **`question_template_weights`（新表）**——偏好学习闭环的账本：一个模板一行，
//!    记权重、是否被静音、正负样本各多少、最后一次学是什么时候。
//!    它是"越用越懂你"的唯一持久化落点（纯计数与权重，可单测、可解释、零 LLM）。
//! 2. **`fragments.last_asked_at`（新列）**——上次被问出的时刻。
//!    与既有的 `used_count` 分工：次数管"问过多少回"，时刻管"上次是多久以前"——
//!    新颖度随时间回收要用后者（差一天与差一个月，该不该再问完全不同）。
//! 3. **`fragments.auto_derived`（新列）**——这条卡是不是**系统自动派生**出来的。
//!    它决定引力里的派生折扣打不打（只打自动的，不约束作者手动顺着灵感再问），
//!    以及自动派生链深要不要卡上限。
//!
//! # 为什么新列走 `prepare`
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能**先查 `PRAGMA table_info`** 再决定
//! （迁移铁律第 3 条：幂等化）——升级到一半断电/被杀之后重跑不会撞车。

use rusqlite::Connection;

use crate::error::Result;

/// v9 的静态步骤：只建表（本来就带 `IF NOT EXISTS`，可以静态跑）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS question_template_weights (
        template_key TEXT    PRIMARY KEY,
        weight        REAL    NOT NULL DEFAULT 1.0,
        enabled       INTEGER NOT NULL DEFAULT 1,
        positives     INTEGER NOT NULL DEFAULT 0,
        negatives     INTEGER NOT NULL DEFAULT 0,
        updated_at    INTEGER NOT NULL
    )",
];

/// 两个新列**先查再决定**——照 v3~v6/v8 的样子，重跑不撞车。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "fragments", "last_asked_at")? {
        steps.push("ALTER TABLE fragments ADD COLUMN last_asked_at INTEGER".to_string());
    }
    if !column_exists(conn, "fragments", "auto_derived")? {
        steps.push(
            "ALTER TABLE fragments ADD COLUMN auto_derived INTEGER NOT NULL DEFAULT 0".to_string(),
        );
    }
    Ok(steps)
}

/// `table` 上有没有 `column`（`PRAGMA table_info` 的列名在第二列）。
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
