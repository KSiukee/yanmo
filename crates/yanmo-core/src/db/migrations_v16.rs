//! 迁移 v16：**大纲快照**——`outline_snapshots` 一张表。
//!
//! ⚠️ 与 v1~v15 同样**一旦发布不可再改**。
//!
//! # 为什么单独一张表，不塞进 `snapshots`
//!
//! `snapshots` 存的是**正文**（一列 `body`，回滚那条路把它原样写回正文）。
//! 大纲是另一族数据（章纲的一句话 / 四格 / 出场人物），塞进同一个 `body` 列之后，
//! 回滚的人必须先分辨"这一条是正文还是大纲"，分错一次就把大纲写进正文里。
//! 两张表、两条路，谁也不会被谁写脏。
//!
//! # 为什么没有外键
//!
//! 章节真删（清空回收站）之后这份留底还要能看——它记的是"改之前是什么样"。
//! 外键级联会在删节点那一刻把留底一并抹掉，正是最需要它的时候没了。
//! 清理由留底自己的滚动保留做（每章留最近几份）。
//!
//! # 存的是什么
//!
//! `payload` 是一份 JSON：这一章的章纲（一句话）、四格、以及出场人物 id 列表。
//! 它是**只读快照**，不参与任何查询条件——放在一列里比拆成五张表更不容易写坏。

use rusqlite::Connection;

use crate::error::Result;

/// 建表与索引（都 `IF NOT EXISTS`：迁移要能重跑）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS outline_snapshots (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        node_id    INTEGER NOT NULL,
        work_id    INTEGER NOT NULL,
        payload    TEXT    NOT NULL,
        note       TEXT    NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_outline_snapshots_node
        ON outline_snapshots(node_id, created_at DESC)",
];

/// v16 没有静态之外的步骤。
pub fn prepare(_conn: &Connection) -> Result<Vec<String>> {
    Ok(Vec::new())
}
