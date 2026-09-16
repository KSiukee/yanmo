//! 迁移 v12：**伏笔**（`foreshadows` 新表）+ **事件的故事时间**（`fragments` 三个新列）。
//!
//! ⚠️ 与 v1~v11 同样**一旦发布不可再改**。
//!
//! # 伏笔为什么另起一张表
//!
//! 它有两个**章节锚点**（埋在哪、收在哪）与一条小生命周期（埋着 / 收了 / 不写了）——
//! `fragments` 装得下"一句话 + 元数据"，装不下这个形状；硬塞得再补两列 + 一套状态，
//! 那已经不是碎片了（理由全文见 `model::foreshadow`）。
//!
//! # 故事时间为什么加在 `fragments` 上
//!
//! 与 v8/v9 给问题卡补 `template_key` / `last_asked_at` / `auto_derived` 是同一条思路：
//! `fragments` 就是**统一表**，按 `frag_kind` 分家，种类专有的列直接挂在表上
//! （统一表里已经有三个只有问题卡才用的列）。事件的故事时间同理——
//! 它是**事件那一种**的属性，别的种类读出来就是默认值（空串 / 空 / 假）。
//!
//! 三列的语义：
//!
//! - `story_time`：自由文本（`承平三年·春`），**给人看**，核心不认识内容；
//! - `story_order`：整数排序值（"故事开始后第几天"），**给排序用**，可空；
//! - `flashback`：倒叙 / 回忆的标记（顺序检查跳过它）——不勾会被误报成倒置。
//!
//! # 新列走 `prepare`
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能先查 `PRAGMA table_info` 再决定
//! （迁移铁律第 3 条：幂等化）——升级到一半断电 / 被杀之后重跑不会撞车。

use rusqlite::Connection;

use crate::error::Result;

/// v12 的静态步骤：建表 + 建索引（都幂等，可以静态跑）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS foreshadows (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        work_id        INTEGER NOT NULL REFERENCES works(id) ON DELETE CASCADE,
        body           TEXT    NOT NULL,
        planted_node   INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
        collected_node INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
        state          TEXT    NOT NULL,
        note           TEXT    NOT NULL DEFAULT '',
        created_at     INTEGER NOT NULL,
        updated_at     INTEGER NOT NULL,
        deleted_at     INTEGER
    )",
    // 体检最常问的是"这本书还埋着的有哪些"：状态进索引
    "CREATE INDEX IF NOT EXISTS idx_foreshadows_work
        ON foreshadows(work_id, state, deleted_at)",
];

/// 三个新列**先查再决定**——照 v3~v6/v8/v9 的样子，重跑不撞车。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "fragments", "story_time")? {
        steps.push("ALTER TABLE fragments ADD COLUMN story_time TEXT NOT NULL DEFAULT ''".to_string());
    }
    if !column_exists(conn, "fragments", "story_order")? {
        steps.push("ALTER TABLE fragments ADD COLUMN story_order INTEGER".to_string());
    }
    if !column_exists(conn, "fragments", "flashback")? {
        steps.push("ALTER TABLE fragments ADD COLUMN flashback INTEGER NOT NULL DEFAULT 0".to_string());
    }
    Ok(steps)
}

/// `table` 上有没有 `column`（`PRAGMA table_info` 的列名在第二列）。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_is_a_single_statement() {
        for (i, s) in STEPS.iter().enumerate() {
            let t = s.trim();
            assert!(t.ends_with(')') || !t.contains(';'), "步骤 {i} 可能含多条语句：{t}");
            assert!(!t.contains(';'), "步骤 {i} 含分号（多语句）——迁移必须逐条 execute：{t}");
        }
    }
}
