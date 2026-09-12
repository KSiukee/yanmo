//! 迁移 v3：版本快照的**滚动保留**标记（`snapshots.pinned`）。
//!
//! ⚠️ 与 v1/v2 同样**一旦发布不可再改**。
//!
//! # 为什么加一列，而不是另建表
//!
//! `snapshots` 已经有两个写入者（抢救快照 / 关窗快照）。滚动保留、回滚与差异对比
//! 都在**同一张表**上做，所以这里只补一个区分维度：
//!
//! - `pinned = 0`：自动快照（关窗 / 抢救 / 回滚前留底）——**滚动删除只清这一批**；
//! - `pinned = 1`：作者亲手留的版本——**永远不参与滚动删除**。
//!
//! 两者的正文来源与结构完全一样，分两张表只会让"这一章有哪些版本"变成一次 union。
//!
//! # 为什么静态步骤是空的
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能**先查 `PRAGMA table_info`** 再决定。
//! 这一步放进 [`prepare`]：框架在事务内、静态步骤之前调用它——
//! 迁移铁律第 3 条（幂等化）要的就是这个。

use rusqlite::Connection;

use crate::error::Result;

/// v3 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

/// 要加哪些列，**先查再决定**——重跑不会撞"duplicate column name"。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    if column_exists(conn, "snapshots", "pinned")? {
        return Ok(Vec::new());
    }
    // 存量快照（关窗 / 抢救留下的）一律按"自动"处理：它们本来就是自动写的
    Ok(vec!["ALTER TABLE snapshots ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0".to_string()])
}

/// `table` 上有没有 `column`（`PRAGMA table_info` 的列名在第二列）。
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // 表名是常量字面量，不是外部输入——PRAGMA 不支持参数占位，只能拼
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshots_columns(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("PRAGMA table_info(snapshots)").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn adds_pinned_once_and_then_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::open(dir.path().join("v3.db")).unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        assert!(snapshots_columns(&conn).contains(&"pinned".to_string()), "v3 应给快照表加上 pinned");
        // 再问一次：列已在，就不能再产出 ALTER（否则重复执行会报 duplicate column）
        assert!(prepare(&conn).unwrap().is_empty(), "列已存在时不应再产出加列语句");
    }
}
