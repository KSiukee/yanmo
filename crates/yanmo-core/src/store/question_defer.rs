//! 延后队列（**读侧**）：这本书还等着什么、某张卡延后过几回、每张卡各几次。
//!
//! 一次延后一行（`question_deferrals`）：条件是什么、作者当时填了什么、什么时候重出的，
//! 全留着。于是"同一问题累计延后几次"不用另立一处账，"这张卡在等什么"也随时读得回来。
//!
//! 写侧（延后、重出）在 [`super::question_defer_write`]。

use std::collections::HashMap;

use rusqlite::params;
use serde::Serialize;

use super::{Store, KIND_QUESTION};
use crate::error::Result;
use crate::question::DeferKind;

/// 一条延后记录。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Deferral {
    pub id: i64,
    pub card_id: i64,
    pub kind: DeferKind,
    /// 时间条件的时刻（别的类型为空）
    pub due_at_ms: Option<i64>,
    /// 写到哪个节点（"写到"类条件才有）
    pub anchor_node: Option<i64>,
    /// 作者自己填的那句「什么时候再问我」（作者数据；空串＝没填）
    pub note: String,
    pub created_at: i64,
    /// 什么时候重出的；`None` = 还等着
    pub resolved_at: Option<i64>,
}

/// 读延后记录用的列（**都带表别名**：这条查询要 join 碎片表，不限定就撞名了）。
const COLS: &str = "d.id, d.card_id, d.kind, d.due_at_ms, d.anchor_node, d.note, d.created_at, \
                    d.resolved_at";

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, i64, String, Option<i64>, Option<i64>, String, i64, Option<i64>)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn into_deferral(
    raw: (i64, i64, String, Option<i64>, Option<i64>, String, i64, Option<i64>),
) -> Result<Deferral> {
    Ok(Deferral {
        id: raw.0,
        card_id: raw.1,
        kind: DeferKind::parse(&raw.2)?,
        due_at_ms: raw.3,
        anchor_node: raw.4,
        note: raw.5,
        created_at: raw.6,
        resolved_at: raw.7,
    })
}

impl Store {
    /// 这本书里**还等着**的延后（界面显示"它在等什么"，重出扫描也用它）。
    pub fn open_deferrals(&self, work_id: i64) -> Result<Vec<Deferral>> {
        let sql = format!(
            "SELECT {COLS} FROM question_deferrals d
               JOIN fragments f ON f.id = d.card_id
              WHERE f.work_id = ?1 AND f.frag_kind = ?2 AND f.deleted_at IS NULL
                AND d.resolved_at IS NULL
              ORDER BY d.created_at, d.id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id, KIND_QUESTION], read_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_deferral(row?)?);
        }
        Ok(out)
    }

    /// 一张卡的延后历史（含已重出的那些）——"它被延后过几回、都因为什么"。
    pub fn card_deferrals(&self, card_id: i64) -> Result<Vec<Deferral>> {
        let sql = format!(
            "SELECT {COLS} FROM question_deferrals d
              WHERE d.card_id = ?1 ORDER BY d.created_at, d.id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![card_id], read_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_deferral(row?)?);
        }
        Ok(out)
    }

    /// 每张卡被延后过几次——**防死循环的账**（选题时给引力打折用）。
    pub(super) fn defer_counts(&self, work_id: i64) -> Result<HashMap<i64, i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.card_id, COUNT(*) FROM question_deferrals d
               JOIN fragments f ON f.id = d.card_id
              WHERE f.work_id = ?1 AND f.frag_kind = ?2 AND f.deleted_at IS NULL
              GROUP BY d.card_id",
        )?;
        let rows = stmt.query_map(params![work_id, KIND_QUESTION], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (card_id, count) = row?;
            out.insert(card_id, count);
        }
        Ok(out)
    }
}
