//! 问题卡的读写：**碎片统一表**（`fragments`）里 `frag_kind = 'question'` 的那一种。
//!
//! 不另起一张表：`fragments` 从 v1 起就是事件 / 灵感速记 / 口述段落 / 答案池 / 问题卡的同一张表，
//! 复用它就自动带上 `source / created_at / used_count / linked / importance / derived_from`，
//! 也守住"改数据只有一个入口"（另起一张＝第二套存储）。状态迁移在 [`super::card_move`]。

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{NewQuestionCard, QuestionCard, QuestionState};
use crate::time::now_millis;

/// 问题卡在碎片统一表里的种类码（写进 `fragments.frag_kind`；**别改**）。
pub const KIND_QUESTION: &str = "question";

/// 读卡用的列（顺序与 [`read_raw`] 一一对应）。
const COLS: &str = "id, work_id, body, source, template_key, status, importance, used_count, \
                    linked, derived_from, created_at, updated_at";

/// 库里的一行原样读出来（`status` 还是字符串，认不认识交给 [`into_card`]）。
struct RawCard {
    id: i64,
    work_id: Option<i64>,
    body: String,
    source: String,
    template_key: String,
    status: String,
    importance: f64,
    used_count: i64,
    linked: String,
    derived_from: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

fn read_raw(row: &Row<'_>) -> rusqlite::Result<RawCard> {
    Ok(RawCard {
        id: row.get(0)?,
        work_id: row.get(1)?,
        body: row.get(2)?,
        source: row.get(3)?,
        template_key: row.get(4)?,
        status: row.get(5)?,
        importance: row.get(6)?,
        used_count: row.get(7)?,
        linked: row.get(8)?,
        derived_from: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

/// 把一行变成一张卡：认不出来的状态**如实报错**，不猜一个"像那么回事"的态。
fn into_card(raw: RawCard) -> Result<QuestionCard> {
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
        linked: raw.linked,
        derived_from: raw.derived_from,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}

fn missing(id: i64) -> Error {
    Error::invalid_with(codes::CARD_NOT_FOUND, [("card_id", id.to_string())])
}

/// 这本书在不在（软删的也算不在）：往一本已经进回收站的书里塞卡没有意义。
fn work_alive(conn: &Connection, work_id: i64) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM works WHERE id = ?1 AND deleted_at IS NULL)",
        params![work_id],
        |r| r.get(0),
    )?)
}

impl Store {
    /// 新建一张问题卡：状态从「待问」开始，落库与留痕**同一个事务**。
    ///
    /// 校验都在写之前：正文不许空、重要度 0~1、派生来源必须是**同一本书**里的一张卡。
    pub fn create_question_card(&mut self, new: &NewQuestionCard) -> Result<i64> {
        if new.body.trim().is_empty() {
            return Err(Error::invalid(codes::CARD_BODY_EMPTY));
        }
        if !new.importance.is_finite() || !(0.0..=1.0).contains(&new.importance) {
            return Err(Error::invalid_with(
                codes::CARD_IMPORTANCE_OUT_OF_RANGE,
                [("value", new.importance.to_string())],
            ));
        }
        let tx = self.conn.transaction()?;
        if !work_alive(&tx, new.work_id)? {
            return Err(Error::invalid_with(
                codes::WORK_GONE,
                [("work_id", new.work_id.to_string())],
            ));
        }
        if let Some(source_card) = new.derived_from {
            let owner: Option<i64> = tx
                .query_row(
                    "SELECT work_id FROM fragments
                      WHERE id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL",
                    params![source_card, KIND_QUESTION],
                    |r| r.get(0),
                )
                .optional()?;
            if owner != Some(new.work_id) {
                return Err(Error::invalid_with(
                    codes::CARD_DERIVED_FROM_INVALID,
                    [
                        ("derived_from", source_card.to_string()),
                        ("work_id", new.work_id.to_string()),
                    ],
                ));
            }
        }

        let now = now_millis();
        tx.execute(
            "INSERT INTO fragments(work_id, frag_kind, body, source, status, importance,
                                   used_count, derived_from, template_key, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?9)",
            params![
                new.work_id,
                KIND_QUESTION,
                new.body,
                new.source,
                QuestionState::Pending.as_str(),
                new.importance,
                new.derived_from,
                new.template_key,
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            id,
            "create",
            json!({
                "kind": KIND_QUESTION,
                "to": QuestionState::Pending.as_str(),
                "trigger": "create",
                "source": new.source,
                "template_key": new.template_key,
                "derived_from": new.derived_from,
            }),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 取一张卡（软删的不算）。
    pub fn question_card(&self, id: i64) -> Result<QuestionCard> {
        let sql = format!(
            "SELECT {COLS} FROM fragments
              WHERE id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL"
        );
        let raw = self
            .conn
            .query_row(&sql, params![id, KIND_QUESTION], read_raw)
            .optional()?
            .ok_or_else(|| missing(id))?;
        into_card(raw)
    }

    /// 一本书里的问题卡，可按状态筛（`None` = 全部）；按创建顺序给。
    pub fn question_cards(
        &self,
        work_id: i64,
        state: Option<QuestionState>,
    ) -> Result<Vec<QuestionCard>> {
        let sql = format!(
            "SELECT {COLS} FROM fragments
              WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
                AND (?3 IS NULL OR status = ?3)
              ORDER BY created_at, id"
        );
        let wanted = state.map(QuestionState::as_str);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id, KIND_QUESTION, wanted], read_raw)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_card(row?)?);
        }
        Ok(out)
    }
}
