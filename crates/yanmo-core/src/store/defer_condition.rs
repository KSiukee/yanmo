//! 延后条件的**判据**：这条延后到点了没有、写到地方了没有。
//!
//! 判据与"什么时候该扫"分开（那个在 [`super::question_defer_write`]）：
//! 规则本身要能单独读、单独测，不必拉上事务与写库。
//!
//! 三条口径：
//!
//! - 时间：到点；
//! - 写到：锚点**子树里出现正文**（"这一章写完了"与"进到某一卷了"是同一条判据，
//!   差别只在锚点是章还是卷）；
//! - 只有作者：永不自动重出；
//! - **锚点已经不在书里了 → 算翻篇**（条件当作满足）：让作者删掉的那件事永远堵着一条问题，
//!   比"多问一次"更糟——而且堵着的那个才是真的死路；
//! - 条件字段缺了（库被人手改坏）同样按翻篇处理：宁可多问一次，不留死路。

use rusqlite::{params, Connection};

use super::question_defer::Deferral;
use crate::error::Result;
use crate::question::DeferKind;

/// 一条延后的条件满足了没有。
pub(super) fn satisfied(conn: &Connection, deferral: &Deferral, now_ms: i64) -> Result<bool> {
    match deferral.kind {
        // 只有作者：永不自动重出（"我自己想起来再问"）
        DeferKind::Manual => Ok(false),
        DeferKind::Time => Ok(deferral.due_at_ms.is_none_or(|due| due <= now_ms)),
        DeferKind::Written => match deferral.anchor_node {
            None => Ok(true),
            Some(node) => written_satisfied(conn, node),
        },
    }
}

/// 「写到这个节点」：它自己或它的子孙里**出现正文**。
fn written_satisfied(conn: &Connection, anchor_node: i64) -> Result<bool> {
    let alive: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM nodes WHERE id = ?1 AND deleted_at IS NULL)",
        params![anchor_node],
        |r| r.get(0),
    )?;
    if !alive {
        return Ok(true); // 那一章/那一卷已经不在了：这件事翻篇
    }
    Ok(conn.query_row(
        "WITH RECURSIVE sub(id) AS (
             SELECT id FROM nodes WHERE id = ?1
             UNION ALL
             SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
         )
         SELECT EXISTS(
             SELECT 1 FROM nodes n JOIN sub ON sub.id = n.id
             LEFT JOIN node_contents c ON c.node_id = n.id
             WHERE n.deleted_at IS NULL AND c.body IS NOT NULL AND c.body <> '')",
        params![anchor_node],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_never_fires_and_written_without_anchor_is_settled() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let manual = Deferral {
            id: 1,
            card_id: 1,
            kind: DeferKind::Manual,
            due_at_ms: None,
            anchor_node: None,
            note: String::new(),
            created_at: 0,
            resolved_at: None,
        };
        assert!(!satisfied(&conn, &manual, i64::MAX).unwrap(), "只有作者：永不自动重出");

        let time = Deferral { kind: DeferKind::Time, due_at_ms: Some(100), ..manual.clone() };
        assert!(!satisfied(&conn, &time, 99).unwrap());
        assert!(satisfied(&conn, &time, 100).unwrap(), "到点即满足");
        let broken = Deferral { kind: DeferKind::Time, due_at_ms: None, ..manual.clone() };
        assert!(satisfied(&conn, &broken, 0).unwrap(), "字段缺了按翻篇：宁可多问一次，不留死路");

        let written = Deferral { kind: DeferKind::Written, anchor_node: None, ..manual };
        assert!(satisfied(&conn, &written, 0).unwrap(), "没有锚点：这件事翻篇");
    }
}
