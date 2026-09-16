//! 问题卡的读写：**碎片统一表**（`fragments`）里 `frag_kind = 'question'` 的那一种。
//!
//! 不另起一张表：`fragments` 从 v1 起就是事件 / 灵感速记 / 口述段落 / 答案池 / 问题卡的同一张表，
//! 复用它就自动带上 `source / created_at / used_count / linked / importance / derived_from`，
//! 也守住"改数据只有一个入口"（另起一张＝第二套存储）。状态迁移在 [`super::card_move`]，
//! 行的映射在 [`super::card_row`]，选题与偏好分别在 [`super::question_select`] /
//! [`super::question_weights`]。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::card_row::{into_card, missing, read_raw, COLS};
use super::question_pool::validate_derivation;
use super::{work_alive, Store};
use crate::error::{codes, Error, Result};
use crate::gravity::AttractorParams;
use crate::model::{FragmentKind, NewQuestionCard, QuestionCard, QuestionState};
use crate::time::now_millis;

/// 问题卡在碎片统一表里的种类码（写进 `fragments.frag_kind`；**别改**）。
///
/// 取值本身归 [`FragmentKind`] 那一处管（"有哪些种类"只该有一个地方知道）——
/// 这里留一个名字，是因为全仓都在 [`Self`] 这条线上引用它。
pub const KIND_QUESTION: &str = FragmentKind::Question.as_str();

impl Store {
    /// 新建一张问题卡：状态从「待问」开始，落库与留痕**同一个事务**。
    ///
    /// 校验都在写之前：正文不许空、重要度 0~1、派生关系成立（来源是**同一本书**里的卡；
    /// 系统自动派生的还不得越过链深上限——那两问在 `question_pool::validate_derivation`）。
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
        validate_derivation(
            &tx,
            new.work_id,
            new.derived_from,
            new.auto_derived,
            AttractorParams::default().max_derived_depth,
        )?;

        let now = now_millis();
        // 锚点按 JSON 数组存（与别的碎片同一种写法）；序列化不了就是空数组，不该为它挡住建卡
        let linked = serde_json::to_string(&new.linked).unwrap_or_else(|_| "[]".to_string());
        tx.execute(
            "INSERT INTO fragments(work_id, frag_kind, body, source, status, importance,
                                   used_count, last_asked_at, linked, derived_from, auto_derived,
                                   template_key, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, 0, NULL, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                new.work_id,
                KIND_QUESTION,
                new.body,
                new.source,
                QuestionState::Pending.as_str(),
                new.importance,
                linked,
                new.derived_from,
                i64::from(new.auto_derived),
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
                "auto_derived": new.auto_derived,
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
            .query_row(&sql, params![id, KIND_QUESTION], |row| read_raw(row))
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
        let rows = stmt.query_map(params![work_id, KIND_QUESTION, wanted], |row| read_raw(row))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_card(row?)?);
        }
        Ok(out)
    }
}
