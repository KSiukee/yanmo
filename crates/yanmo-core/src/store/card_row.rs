//! 碎片统一表里「问题卡那一行」↔ 领域对象的映射。
//!
//! 单独放在一处：列的顺序、认不出来的状态怎么办——只该有这一个地方知道。
//! 读一处、写一处（[`super::card`] 的建卡与 [`super::card_move`] 的迁移都走这里读）。

use rusqlite::Row;

use crate::error::{codes, Error, Result};
use crate::model::{QuestionCard, QuestionState};

/// 读卡用的列（顺序与 [`read_raw`] 一一对应）。
pub(super) const COLS: &str = "id, work_id, body, source, template_key, status, importance, \
                               used_count, last_asked_at, linked, derived_from, auto_derived, \
                               created_at, updated_at";

/// 库里的一行原样读出来（`status` 还是字符串，认不认识交给 [`into_card`]）。
pub(super) struct RawCard {
    id: i64,
    work_id: Option<i64>,
    body: String,
    source: String,
    template_key: String,
    status: String,
    importance: f64,
    used_count: i64,
    last_asked_at: Option<i64>,
    linked: String,
    derived_from: Option<i64>,
    auto_derived: i64,
    created_at: i64,
    updated_at: i64,
}

pub(super) fn read_raw(row: &Row<'_>) -> rusqlite::Result<RawCard> {
    Ok(RawCard {
        id: row.get(0)?,
        work_id: row.get(1)?,
        body: row.get(2)?,
        source: row.get(3)?,
        template_key: row.get(4)?,
        status: row.get(5)?,
        importance: row.get(6)?,
        used_count: row.get(7)?,
        last_asked_at: row.get(8)?,
        linked: row.get(9)?,
        derived_from: row.get(10)?,
        auto_derived: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

/// 把一行变成一张卡：认不出来的状态**如实报错**，不猜一个"像那么回事"的态。
pub(super) fn into_card(raw: RawCard) -> Result<QuestionCard> {
    Ok(QuestionCard {
        id: raw.id,
        // 库被人手改成没有归属的卡按作品 0 算：按书列卡列不到它，但取单卡仍取得出来
        work_id: raw.work_id.unwrap_or(0),
        body: raw.body,
        source: raw.source,
        template_key: raw.template_key,
        state: QuestionState::parse(&raw.status)?,
        importance: raw.importance,
        used_count: raw.used_count,
        last_asked_at: raw.last_asked_at,
        linked: raw.linked,
        derived_from: raw.derived_from,
        auto_derived: raw.auto_derived != 0,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}

/// 「卡不存在」这一条：只有一处说法，免得两种口径在同一条链上打架。
pub(super) fn missing(id: i64) -> Error {
    Error::invalid_with(codes::CARD_NOT_FOUND, [("card_id", id.to_string())])
}
