//! 迁移 v5：节点上的**三个字数口径**（`nodes.char_count` / `nodes.chars_no_punct`）。
//!
//! ⚠️ 与 v1~v4 同样**一旦发布不可再改**。
//!
//! # 为什么加在 nodes 上
//!
//! 目录树、卷/全书合计、书架都要"一眼看字数"，所以这些数是**预聚合**存在节点上的
//! （`nodes.word_count` 一直是"按词"那一档，写入时回算）。作者现在能在状态栏选口径了，
//! 这三处也得跟着——要么为每个口径各存一列，要么每次显示都去扫正文（长篇上不可能）。
//!
//! 所以：**三个口径各一列**，`word_count` 继续当"按词"（不动含义、老代码不会读错）。
//!
//! # 存量怎么办
//!
//! 新加的两列默认 0，**光靠迁移填不上**：CJK 口径在 SQL 里算不出来（不是简单的长度）。
//! 所以迁移只加列，真正的重算走 [`Store::backfill_node_counts`]（Rust 过一遍正文，
//! 只在没做过时跑一次，跑完留标记）。
//!
//! # 为什么静态步骤是空的
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能**先查 `PRAGMA table_info`** 再决定
//! （迁移铁律第 3 条：幂等化）——照 v3/v4 的样子放进 [`prepare`]。

use rusqlite::Connection;

use crate::error::Result;

/// v5 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

/// 要加哪些列，**先查再决定**——重跑不会撞 "duplicate column name"。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    for column in ["char_count", "chars_no_punct"] {
        if !column_exists(conn, "nodes", column)? {
            steps.push(format!(
                "ALTER TABLE nodes ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"
            ));
        }
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
