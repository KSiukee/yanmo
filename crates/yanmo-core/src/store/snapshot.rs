//! 版本快照（`snapshots` 表）：**最危险的时候先留一份**。
//!
//! 现在负责两类：**抢救快照**（内存态与库不一致、落盘卡住）与**关窗快照**。
//! 滚动保留、回滚、差异对比属于后续的版本快照能力，**在同一张表上做，不另建表**。
//!
//! 「变则快照」是刻意的：内容没变就不留新行——关窗是高频动作，
//! 每关一次都堆一行的话，那张表很快就没用了。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{ContentStats, Store};
use crate::error::Result;
use crate::text;
use crate::time::now_millis;

impl Store {
    /// **紧急快照**：内存里的正文先落一条快照（保证这份字不丢），再把库里的正文改回内存态。
    ///
    /// 顺序是有意的：先保险，再修复。任何一步失败都不会让"手上这份字"消失。
    pub fn emergency_snapshot(&mut self, node_id: i64, body: &str, reason: &str) -> Result<ContentStats> {
        self.node_work(node_id)?;
        self.write_snapshot(node_id, body, reason)?;
        self.record("snapshots", node_id, "emergency", json!({ "reason": reason }))?;
        self.write_body(node_id, body)
    }

    /// 内容变了才留快照（**幂等**）。返回是否真的写了新快照。
    pub fn snapshot_if_changed(&mut self, node_id: i64, reason: &str) -> Result<bool> {
        self.node_work(node_id)?;
        let body = self.read_body(node_id)?;
        let latest: Option<String> = self
            .conn
            .query_row(
                "SELECT body FROM snapshots WHERE node_id = ?1 ORDER BY id DESC LIMIT 1",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(previous) = latest {
            if text::content_hash(&previous) == text::content_hash(&body) {
                return Ok(false);
            }
        }
        self.write_snapshot(node_id, &body, reason)?;
        self.record("snapshots", node_id, "snapshot", json!({ "reason": reason }))?;
        Ok(true)
    }

    fn write_snapshot(&self, node_id: i64, body: &str, reason: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO snapshots(node_id, body, char_count, reason, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![node_id, body, text::count_chars(body), reason, now_millis()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}
