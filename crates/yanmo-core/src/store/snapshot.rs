//! 版本快照（`snapshots` 表）：**最危险的时候先留一份**，也给作者一颗后悔药。
//!
//! 三类写入者共用这一张表（**不另建表**）：
//!
//! - **抢救快照**：内存态与库不一致、落盘卡住（`reason = stuck / desync`）；
//! - **关窗快照**：正常退出前留一份（`reason = close`）；**幂等：内容没变不留新行**；
//! - **手动版本**：作者亲手留的（`reason = keep`，`pinned = 1`）——
//!   **滚动删除永远不碰它**；回滚前还会自动留一份 `before_restore`（自动类，可滚动）。
//!
//! 「变则快照」是刻意的：内容没变就不留新行——关窗是高频动作，
//! 每关一次都堆一行的话，那张表很快就没用了。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{ContentStats, Store};
use crate::error::{codes, Error, Result};
use crate::text;
use crate::time::now_millis;

/// 每章**自动**快照的滚动保留份数。手动留的版本不计入、也不会被清。
pub const AUTO_SNAPSHOTS_KEPT: i64 = 20;

/// 一条快照的摘要——**不带正文**（正文按需再拉，和目录树同一条懒加载纪律）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSummary {
    pub id: i64,
    pub char_count: i64,
    /// 写入理由：close / stuck / desync / keep / before_restore（界面自己查字典渲染）
    pub reason: String,
    /// 作者亲手留的版本（不参与滚动删除）
    pub pinned: bool,
    pub created_at: i64,
}

impl Store {
    /// **紧急快照**：内存里的正文先落一条快照（保证这份字不丢），再把库里的正文改回内存态。
    ///
    /// 顺序是有意的：先保险，再修复。任何一步失败都不会让"手上这份字"消失。
    pub fn emergency_snapshot(&mut self, node_id: i64, body: &str, reason: &str) -> Result<ContentStats> {
        self.node_work(node_id)?;
        self.write_snapshot(node_id, body, reason, false)?;
        self.record("snapshots", node_id, "emergency", json!({ "reason": reason }))?;
        self.write_body(node_id, body)
    }

    /// 内容变了才留快照（**幂等**）。返回是否真的写了新快照。
    pub fn snapshot_if_changed(&mut self, node_id: i64, reason: &str) -> Result<bool> {
        self.node_work(node_id)?;
        let body = self.read_body(node_id)?;
        if self.latest_matches(node_id, &body)? {
            return Ok(false);
        }
        self.write_snapshot(node_id, &body, reason, false)?;
        self.record("snapshots", node_id, "snapshot", json!({ "reason": reason }))?;
        Ok(true)
    }

    /// 这一章有哪些版本（**新的在前**，只给摘要）。
    pub fn list_snapshots(&self, node_id: i64) -> Result<Vec<SnapshotSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, char_count, reason, pinned, created_at
               FROM snapshots WHERE node_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![node_id], |row| {
            Ok(SnapshotSummary {
                id: row.get(0)?,
                char_count: row.get(1)?,
                reason: row.get(2)?,
                pinned: row.get::<_, i64>(3)? != 0,
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 某一条快照的正文（预览差异时才拉）。
    pub fn snapshot_body(&self, snapshot_id: i64) -> Result<String> {
        self.conn
            .query_row("SELECT body FROM snapshots WHERE id = ?1", params![snapshot_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::invalid_with(codes::SNAPSHOT_NOT_FOUND, [("snapshot_id", snapshot_id.to_string())]))
    }

    /// **手动留一版**（`pinned`）：内容与最新一份相同就不重复留，返回新快照 id。
    pub fn keep_snapshot(&mut self, node_id: i64) -> Result<Option<i64>> {
        self.node_work(node_id)?;
        let body = self.read_body(node_id)?;
        if self.latest_matches(node_id, &body)? {
            return Ok(None);
        }
        let id = self.write_snapshot(node_id, &body, "keep", true)?;
        self.record("snapshots", node_id, "keep", json!({ "snapshot_id": id }))?;
        Ok(Some(id))
    }

    /// 删掉一条快照（作者不要这一版了）。正文一个字都不动。
    pub fn drop_snapshot(&mut self, snapshot_id: i64) -> Result<()> {
        let node_id = self.snapshot_node(snapshot_id)?;
        self.conn.execute("DELETE FROM snapshots WHERE id = ?1", params![snapshot_id])?;
        self.record("snapshots", node_id, "drop", json!({ "snapshot_id": snapshot_id }))
    }

    /// **回滚**到某一条快照：先把当前这一版留底（`before_restore`），再把正文改回去。
    ///
    /// 顺序同抢救：**先保险，再覆盖**。返回 `(哪一章, 写回的正文, 字数)`——
    /// 界面据此重建落盘基准，并确认回的正是手上这一章。
    pub fn restore_snapshot(&mut self, snapshot_id: i64) -> Result<(i64, String, ContentStats)> {
        let node_id = self.snapshot_node(snapshot_id)?;
        self.node_work(node_id)?; // 节点已进回收站就别回滚（不往回收站里写东西）
        let body = self.snapshot_body(snapshot_id)?;
        self.snapshot_if_changed(node_id, "before_restore")?;
        let stats = self.write_body(node_id, &body)?;
        self.record("snapshots", node_id, "restore", json!({ "snapshot_id": snapshot_id }))?;
        Ok((node_id, body, stats))
    }

    /// 滚动保留：清掉该章**多余**的自动快照，只留最近 `keep` 份；手动版本一份不动。
    pub fn prune_snapshots(&self, node_id: i64, keep: i64) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM snapshots
              WHERE node_id = ?1 AND pinned = 0
                AND id NOT IN (SELECT id FROM snapshots
                                WHERE node_id = ?1 AND pinned = 0
                                ORDER BY id DESC LIMIT ?2)",
            params![node_id, keep.max(0)],
        )?)
    }

    // ── 内部 ────────────────────────────────────────────────────────────

    /// 最新一份快照是不是就是这份内容（关窗幂等与"手动留一版"共用这一处判断）。
    fn latest_matches(&self, node_id: i64, body: &str) -> Result<bool> {
        let latest: Option<String> = self
            .conn
            .query_row(
                "SELECT body FROM snapshots WHERE node_id = ?1 ORDER BY id DESC LIMIT 1",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(latest.is_some_and(|previous| text::content_hash(&previous) == text::content_hash(body)))
    }

    /// 快照属于哪一章（删 / 回滚之前先问出来）。
    fn snapshot_node(&self, snapshot_id: i64) -> Result<i64> {
        self.conn
            .query_row("SELECT node_id FROM snapshots WHERE id = ?1", params![snapshot_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::invalid_with(codes::SNAPSHOT_NOT_FOUND, [("snapshot_id", snapshot_id.to_string())]))
    }

    /// 落一条快照，并**顺手做滚动保留**（写快照的入口只有这一个，谁都绕不过裁剪）。
    fn write_snapshot(&self, node_id: i64, body: &str, reason: &str, pinned: bool) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO snapshots(node_id, body, char_count, reason, pinned, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![node_id, body, text::count_chars(body), reason, i64::from(pinned), now_millis()],
        )?;
        let id = self.conn.last_insert_rowid();
        self.prune_snapshots(node_id, AUTO_SNAPSHOTS_KEPT)?;
        Ok(id)
    }
}
