//! 迁移 v8：碎片统一表上的**问题卡**所需的两个补充。
//!
//! ⚠️ 与 v1~v7 同样**一旦发布不可再改**。
//!
//! # 为什么不是新建一张「问题卡表」
//!
//! `fragments` 从 v1 起就是**碎片统一存储**（事件 / 灵感速记 / 口述段落 / 答案池 / 问题卡同表），
//! 见 v1 的设计约束「碎片统一存储」。问题卡只是 `frag_kind = 'question'` 的一种取值——
//! 另起一张表就等于**第二套存储**，违反单一真相源（仓内铁律：改数据只有一个入口）。
//! 所以这里只补两样表上还没有的东西，不搬表。
//!
//! # 补了哪两样
//!
//! 1. **`template_key`（新列）**——这张卡是哪条问题模板问出来的。
//!    后续的偏好学习闭环靠它把「作答 / 说好 / 舍弃 / 静音」这些信号折算回**同类模板**的权重；
//!    没有它，卡的处置就只能一条一条起作用，学不到"这类问题他不爱答"。
//!    空串＝不是模板产的问题（作者自己写的问题、模块提交的问题）。
//! 2. **状态查询索引**——问题池最常用的问法是「这本书里、还是 pending 的问题有哪些」，
//!    原来的 `idx_fragments_work(work_id, frag_kind, deleted_at)` 只能用到前两列，
//!    `status` 得逐行过滤。补一条覆盖到 `status` 的。
//!
//! # 为什么新列走 `prepare`
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能**先查 `PRAGMA table_info`** 再决定
//! （迁移铁律第 3 条：幂等化）——重跑（升级到一半断电 / 被杀）不会撞 "duplicate column name"。

use rusqlite::Connection;

use crate::error::Result;

/// v8 的静态步骤：只建索引（建索引本来就有 `IF NOT EXISTS`，可以静态跑）。
pub const STEPS: &[&str] =
    &["CREATE INDEX IF NOT EXISTS idx_fragments_state ON fragments(work_id, frag_kind, status)"];

/// 新列**先查再决定**——照 v3~v6 的样子，重跑不撞车。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    if column_exists(conn, "fragments", "template_key")? {
        return Ok(Vec::new());
    }
    Ok(vec![
        "ALTER TABLE fragments ADD COLUMN template_key TEXT NOT NULL DEFAULT ''".to_string(),
    ])
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
