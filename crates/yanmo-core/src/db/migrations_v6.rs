//! 迁移 v6：作品上的**简介**（`works.summary`）与节点上的**一句话**（`nodes.summary`）。
//!
//! ⚠️ 与 v1~v5 同样**一旦发布不可再改**。
//!
//! # 为什么加这两列
//!
//! 投稿包要"正文 + 大纲"，而大纲不能只有一串标题——编辑要看的是**这本书讲什么**
//! （作品简介）与**每一章发生了什么**（每章一句话）。这两样都是**作者手填**的数据，
//! 不是自动摘要（自动抽取是另一条路，现在没做），所以它就是两个普通文本列。
//!
//! # 为什么分开放
//!
//! 简介属于作品（一本书一条）；一句话属于章（只有承载正文的节点才有意义，
//! 但列加在所有节点上——容器写进去也不碍事，读的时候按需取）。
//!
//! # 为什么静态步骤是空的
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能**先查 `PRAGMA table_info`** 再决定
//! （迁移铁律第 3 条：幂等化）——照 v3/v4/v5 的样子放进 [`prepare`]。

use rusqlite::Connection;

use crate::error::Result;

/// v6 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

/// 要加哪些列，**先查再决定**——重跑不会撞 "duplicate column name"。
///
/// 默认空串而不是 NULL：读的地方少一层判空，空串就是"没写过"。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    for table in ["works", "nodes"] {
        if !column_exists(conn, table, "summary")? {
            steps.push(format!("ALTER TABLE {table} ADD COLUMN summary TEXT NOT NULL DEFAULT ''"));
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
