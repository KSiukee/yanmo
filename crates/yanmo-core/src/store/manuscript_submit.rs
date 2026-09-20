//! 投稿记录的**落库**：记一次投稿、改一条、删一条。
//!
//! 状态迁移那一半在 `store::manuscript`（迁移表是唯一真相源，那边过合法性）；这里管的是
//! **投稿历史的账**——同一篇先投甲、被拒、冷冻期过了改投乙，账要一趟一趟记得住。
//!
//! # 为什么"记一次投稿"必须同时改状态
//!
//! 「已投」这个状态的意思就是"投出去了，在等回音"——它不能没有投向与时刻。
//! 所以记投稿与改状态在**同一个事务**里做：要么两样都成，要么两样都没有。
//!
//! # 留痕落在哪
//!
//! - **落字段**：`submissions` 一行 + `works.status`；
//! - **落事件**：op-log `entity='works'`、`op=submit`（payload `{from, to, trigger}`）
//!   与 `entity='submissions'`、`op=record|update|delete`。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::manuscript::{illegal, not_found, set_status_in};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{manuscript_transition, ManuscriptState, Submission, SubmissionResult, SubmitVerdict};
use crate::time::now_millis;

/// 记一次投稿的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionReceipt {
    /// 记之前的状态。
    pub from: ManuscriptState,
    /// 记之后的状态（必是「已投」）。
    pub to: ManuscriptState,
    /// 新写下的那条记录。
    pub submission: Submission,
    /// 记之前算出的判词：不是 `Clear` 时界面本该先说明后果（软拦），核心照样记。
    pub verdict: SubmitVerdict,
}

impl Store {
    /// 记一次投稿：写一条历史 + 把状态推进到「已投」，同一个事务里留痕。
    ///
    /// `submitted_at` 由调用方给（作者可能补记昨天投的）；`cooldown_until` 为空＝没定期限
    /// ——**没期限就不替他假设**（判据里算"还在等"，见 `Submission::is_stale`）。
    ///
    /// 判词只回报不算数：一稿多投是**软拦**，界面先说明后果，作者确认了照样记。
    pub fn record_submission(
        &mut self,
        work_id: i64,
        venue: &str,
        submitted_at: i64,
        cooldown_until: Option<i64>,
        notes: &str,
    ) -> Result<SubmissionReceipt> {
        let venue = venue.trim();
        if venue.is_empty() {
            return Err(Error::invalid(codes::SUBMISSION_VENUE_EMPTY));
        }
        super::work::ensure_alive(&self.conn, work_id)?;
        let from = self.manuscript_status(work_id)?;
        let edge = manuscript_transition(from, ManuscriptState::Submitted)
            .ok_or_else(|| illegal(from, ManuscriptState::Submitted))?;
        // 判词要在写之前算——写完它就把自己的历史改了
        let verdict = self.submission_verdict_for(work_id, venue)?;

        let notes = notes.trim();
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO submissions(work_id, venue, submitted_at, result, cooldown_until,
                                     notes, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                work_id,
                venue,
                submitted_at,
                SubmissionResult::Waiting.as_str(),
                cooldown_until,
                notes,
                now
            ],
        )?;
        let submission_id = tx.last_insert_rowid();
        set_status_in(
            &tx,
            &self.device_id,
            work_id,
            from,
            ManuscriptState::Submitted,
            edge.action,
            "author",
        )?;
        Store::record_in(
            &self.device_id,
            &tx,
            "submissions",
            submission_id,
            "record",
            json!({ "work_id": work_id, "venue": venue, "submitted_at": submitted_at,
                    "cooldown_until": cooldown_until }),
        )?;
        tx.commit()?;
        Ok(SubmissionReceipt {
            from,
            to: ManuscriptState::Submitted,
            submission: Submission {
                id: submission_id,
                work_id,
                venue: venue.to_string(),
                submitted_at,
                result: SubmissionResult::Waiting,
                cooldown_until,
                notes: notes.to_string(),
            },
            verdict,
        })
    }

    /// 改一条投稿记录（记错目标 / 补期限 / 添备注）。
    ///
    /// **结果**不在这里改：它与稿子状态是一回事，得走
    /// [`Store::move_manuscript`](Store::move_manuscript) 一起动。
    pub fn update_submission(
        &mut self,
        submission_id: i64,
        venue: &str,
        submitted_at: i64,
        cooldown_until: Option<i64>,
        notes: &str,
    ) -> Result<()> {
        let venue = venue.trim();
        if venue.is_empty() {
            return Err(Error::invalid(codes::SUBMISSION_VENUE_EMPTY));
        }
        let notes = notes.trim();
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "UPDATE submissions SET venue = ?1, submitted_at = ?2, cooldown_until = ?3,
                                    notes = ?4, updated_at = ?5
              WHERE id = ?6 AND deleted_at IS NULL",
            params![venue, submitted_at, cooldown_until, notes, now, submission_id],
        )?;
        if affected == 0 {
            return Err(not_found(submission_id));
        }
        Store::record_in(
            &self.device_id,
            &tx,
            "submissions",
            submission_id,
            "update",
            json!({ "venue": venue, "submitted_at": submitted_at, "cooldown_until": cooldown_until }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 软删一条投稿记录（记错了）。**还在等回音的那条不许删**——先撤稿再删，
    /// 否则会剩下"状态是已投、却没有对应记录"的空账。
    pub fn delete_submission(&mut self, submission_id: i64) -> Result<()> {
        let row: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT work_id, result FROM submissions WHERE id = ?1 AND deleted_at IS NULL",
                params![submission_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (work_id, result) = row.ok_or_else(|| not_found(submission_id))?;
        let in_use = SubmissionResult::parse(&result)? == SubmissionResult::Waiting
            && self.manuscript_status(work_id)? == ManuscriptState::Submitted;
        if in_use {
            return Err(Error::invalid(codes::SUBMISSION_IN_USE));
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE submissions SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now_millis(), submission_id],
        )?;
        Store::record_in(&self.device_id, &tx, "submissions", submission_id, "delete", json!({}))?;
        tx.commit()?;
        Ok(())
    }
}
