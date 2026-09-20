//! 稿件**状态迁移**的落库：查合法边 → 同事务改 `works.status` → 留一条 op-log。
//!
//! 迁移表在 `model::manuscript_state`（纯逻辑），这里只负责"把它落到库里、并且留下证据"。
//! 记投稿那一族（增 / 改 / 删记录）在 `store::manuscript_submit`——两者的变化理由不一样：
//! 这边每加一条边都要过合法性，那边是投稿历史的账。
//!
//! # 留痕落在哪（验收要的"可观测证据"）
//!
//! - **落字段**：`works.status` + `works.updated_at`；
//! - **落事件**：op-log `entity='works'`、`op=` 动作码（finish / ready / submit / accept / …）、
//!   payload `{from, to, trigger}`。
//!
//! 状态与它对应的那条投稿记录**必须同一个事务**：不然就会出现"稿子标了已投、记录没写上"
//! 这种半截状态（那正是最难查的一类）。
//!
//! # 硬拦只有一条
//!
//! 一稿多投是**软拦**：判词由 `model::submission` 给，界面负责先把后果说清楚，作者确认后
//! 照样能投。核心硬拦的只有"**进已投必须记一次投稿**"——没有投向与时刻的「已投」是空账。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{manuscript_transition, ManuscriptState, SubmissionResult};
use crate::time::now_millis;

impl Store {
    /// 状态迁移总入口（看板拖拽走这条）。
    ///
    /// - 非法边当场拒绝（`manuscript.illegal_transition`），**一个字节都不写**；
    /// - 目标是「已投」时明确报 `manuscript.submit_needs_record`——去走
    ///   [`Store::record_submission`](Store::record_submission)；
    /// - 走到 过稿 / 被拒（含从「待投」补记晚来的回信），或从「已投」把稿子收回来时，
    ///   **同事务**结掉那条还没出结果的记录（过稿 / 退稿 / 自己撤回）：状态与记录一起动。
    ///
    /// 返回迁移**之前**的状态（界面用来说"从 X 到 Y"）。
    pub fn move_manuscript(
        &mut self,
        work_id: i64,
        to: ManuscriptState,
        trigger: &str,
    ) -> Result<ManuscriptState> {
        let from = self.manuscript_status(work_id)?;
        let edge = manuscript_transition(from, to).ok_or_else(|| illegal(from, to))?;
        if to == ManuscriptState::Submitted {
            return Err(Error::invalid(codes::MANUSCRIPT_SUBMIT_NEEDS_RECORD));
        }
        let tx = self.conn.transaction()?;
        match to {
            ManuscriptState::Accepted => {
                close_record_in(&tx, &self.device_id, work_id, SubmissionResult::Accepted)?
            }
            ManuscriptState::Rejected => {
                close_record_in(&tx, &self.device_id, work_id, SubmissionResult::Rejected)?
            }
            // 从「已投」把稿子收回来：那条记录标成"自己撤回"，不然它会一直拦着改投
            ManuscriptState::Ready if from == ManuscriptState::Submitted => {
                close_record_in(&tx, &self.device_id, work_id, SubmissionResult::Withdrawn)?
            }
            _ => {}
        }
        set_status_in(&tx, &self.device_id, work_id, from, to, edge.action, trigger)?;
        tx.commit()?;
        Ok(from)
    }
}

/// 事务内改状态：CAS（当前状态必须还是刚读到的那个）+ 同事务留痕。
///
/// CAS 那一条守的是"读它"与"写它"之间被另一个写者插了一杠：宁可明确报错，
/// 也不把一条非法边悄悄写下去。
pub(super) fn set_status_in(
    tx: &rusqlite::Transaction<'_>,
    device_id: &str,
    work_id: i64,
    from: ManuscriptState,
    to: ManuscriptState,
    action: &str,
    trigger: &str,
) -> Result<()> {
    let now = now_millis();
    let affected = tx.execute(
        "UPDATE works SET status = ?1, updated_at = ?2
          WHERE id = ?3 AND deleted_at IS NULL AND status = ?4",
        params![to.as_str(), now, work_id, from.as_str()],
    )?;
    if affected == 0 {
        // 书没了，或状态在我们读它与写它之间被人改了——两种都要说清是哪一种
        let current: Option<String> = tx
            .query_row(
                "SELECT status FROM works WHERE id = ?1 AND deleted_at IS NULL",
                params![work_id],
                |r| r.get(0),
            )
            .optional()?;
        return match current {
            None => Err(Error::invalid_with(codes::WORK_GONE, [("work_id", work_id.to_string())])),
            Some(current) => Err(illegal_current(&current, to)),
        };
    }
    Store::record_in(
        device_id,
        tx,
        "works",
        work_id,
        action,
        json!({ "from": from.as_str(), "to": to.as_str(), "trigger": trigger }),
    )
}

/// 事务内结掉"还没出结果"的那条记录（过稿 / 退稿 / 自己撤回）。
///
/// 撤回之后再来的回信也要能记上：所以这里找的是 `waiting` **或已标过 `withdrawn`** 的那条。
/// 一条都没有＝这篇从没投过，报 `manuscript.no_open_submission`（不编一条空记录糊上）。
fn close_record_in(
    tx: &rusqlite::Transaction<'_>,
    device_id: &str,
    work_id: i64,
    result: SubmissionResult,
) -> Result<()> {
    let open: Option<i64> = tx
        .query_row(
            "SELECT id FROM submissions
              WHERE work_id = ?1 AND deleted_at IS NULL AND result IN ('waiting', 'withdrawn')
              ORDER BY submitted_at DESC, id DESC LIMIT 1",
            params![work_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(id) = open else {
        return Err(Error::invalid(codes::MANUSCRIPT_NO_OPEN_SUBMISSION));
    };
    tx.execute(
        "UPDATE submissions SET result = ?1, updated_at = ?2 WHERE id = ?3",
        params![result.as_str(), now_millis(), id],
    )?;
    Store::record_in(
        device_id,
        tx,
        "submissions",
        id,
        result.as_str(),
        json!({ "result": result.as_str() }),
    )?;
    Ok(())
}

/// 非法边：`from → to` 不在迁移表里。
pub(super) fn illegal(from: ManuscriptState, to: ManuscriptState) -> Error {
    Error::invalid_with(
        codes::MANUSCRIPT_ILLEGAL_TRANSITION,
        [("from", from.as_str().to_string()), ("to", to.as_str().to_string())],
    )
}

/// 读到的状态与库里的**实际**状态对不上时的同一条错（报真实的那一端）。
fn illegal_current(current: &str, to: ManuscriptState) -> Error {
    Error::invalid_with(
        codes::MANUSCRIPT_ILLEGAL_TRANSITION,
        [("from", current.to_string()), ("to", to.as_str().to_string())],
    )
}

/// 找不到那条投稿记录。
pub(super) fn not_found(submission_id: i64) -> Error {
    Error::invalid_with(
        codes::SUBMISSION_NOT_FOUND,
        [("submission_id", submission_id.to_string())],
    )
}
