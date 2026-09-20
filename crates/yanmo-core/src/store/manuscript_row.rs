//! 稿件流转的**只读那一半**：当前状态、投稿历史、判词、看板一眼。
//!
//! 与写入口分开的理由同问题卡（`card_row` 与 `card_move` 分开）：读的形状与变的理由
//! 不一样——这里随时可以多加一列显示，那里每加一条迁移都要过合法性。

use std::collections::HashMap;

use rusqlite::params;

use super::Store;
use crate::error::Result;
use crate::model::{
    submission_verdict, ManuscriptState, Submission, SubmissionResult, SubmitVerdict, Work,
};
use crate::time::now_millis;

/// 看板的一行：一篇稿子的状态 + 规模 + 在等谁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManuscriptRow {
    /// 作品本身（含流转状态）。
    pub work: Work,
    /// 字数（与书架同一口径：预聚合，不扫正文）。
    pub word_count: i64,
    /// 同上，逐字（含标点）。
    pub char_count: i64,
    /// 同上，逐字（不含标点）——界面按作品的字数口径挑一格显示，与书架同一份口径。
    pub chars_no_punct: i64,
    /// 投过几家（软删的不算）。
    pub submissions: i64,
    /// 还在等回音的那条——卡片上"在等甲刊 · 还差 12 天"就靠它。
    pub active: Option<Submission>,
    /// 最近投给谁（含已出结果的）。
    pub last_venue: Option<String>,
}

impl Store {
    /// 这篇稿子现在到哪一步了。
    pub fn manuscript_status(&self, work_id: i64) -> Result<ManuscriptState> {
        Ok(self.get_work(work_id)?.status)
    }

    /// 这篇的全部投稿记录，新的在前（软删的不算）。
    pub fn submissions(&self, work_id: i64) -> Result<Vec<Submission>> {
        let sql = format!(
            "SELECT {SUB_COLS} FROM submissions
              WHERE work_id = ?1 AND deleted_at IS NULL
              ORDER BY submitted_at DESC, id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id], raw_submission)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(build_submission(row?)?);
        }
        Ok(out)
    }

    /// 记一次新投稿**之前**先看看会不会撞上（只读；判词见 `model::submission`）。
    pub fn submission_verdict_for(&self, work_id: i64, venue: &str) -> Result<SubmitVerdict> {
        Ok(submission_verdict(&self.submissions(work_id)?, venue, now_millis()))
    }

    /// 看板：每篇没进回收站的稿子一行；顺序沿用书架（最近打开优先）。
    ///
    /// 两趟查询查完（作品带规模 + 全部投稿记录），不为每篇各跑一趟。
    pub fn manuscript_board(&self) -> Result<Vec<ManuscriptRow>> {
        let history = self.all_submissions()?;
        let mut out = Vec::new();
        for entry in self.shelf()? {
            let mine = history.get(&entry.work.id);
            out.push(ManuscriptRow {
                word_count: entry.word_count,
                char_count: entry.char_count,
                chars_no_punct: entry.chars_no_punct,
                submissions: mine.map_or(0, |rows| rows.len() as i64),
                active: mine.and_then(|rows| rows.iter().find(|s| s.is_open()).cloned()),
                last_venue: mine.and_then(|rows| rows.first().map(|s| s.venue.clone())),
                work: entry.work,
            });
        }
        Ok(out)
    }

    /// 全部投稿记录按作品归好（新的在前）。
    fn all_submissions(&self) -> Result<HashMap<i64, Vec<Submission>>> {
        let sql = format!(
            "SELECT {SUB_COLS} FROM submissions WHERE deleted_at IS NULL
              ORDER BY work_id, submitted_at DESC, id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], raw_submission)?;
        let mut out: HashMap<i64, Vec<Submission>> = HashMap::new();
        for row in rows {
            let raw = row?;
            let work_id = raw.1;
            out.entry(work_id).or_default().push(build_submission(raw)?);
        }
        Ok(out)
    }
}

/// 投稿记录取哪几列（顺序与 [`SubRow`] 一一对应）。
const SUB_COLS: &str = "id, work_id, venue, submitted_at, result, cooldown_until, notes";

type SubRow = (i64, i64, String, i64, String, Option<i64>, String);

fn raw_submission(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

/// 先取朴素类型，再**在 Rust 侧校验**（不在 SQL 里猜着读）。
fn build_submission(raw: SubRow) -> Result<Submission> {
    Ok(Submission {
        id: raw.0,
        work_id: raw.1,
        venue: raw.2,
        submitted_at: raw.3,
        result: SubmissionResult::parse(&raw.4)?,
        cooldown_until: raw.5,
        notes: raw.6,
    })
}
