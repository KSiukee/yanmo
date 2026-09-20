//! 稿件台命令：看板读数、记一次投稿、改状态、改 / 删投稿记录。
//!
//! 规则全在核心（`store::manuscript*` 与 `model::manuscript_state` / `model::submission`）：
//! 这里只做参数转换 + 把读出来给人用的那份递回去。
//!
//! **判词只说明、不替作者决定**：一稿多投是软拦——界面先拿 [`manuscript_verdict`] 说明后果，
//! 作者确认了再调 [`manuscript_submit`]（核心照样记，并在回执里把判词还回来）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::{ManuscriptState, Submission, SubmitVerdict, DEFAULT_COOLDOWN_DAYS};
use yanmo_core::store::{ManuscriptRow, Store};

/// 一条投稿记录（给人看的那份）。
#[derive(Debug, Serialize)]
pub struct SubmissionDto {
    pub id: i64,
    pub venue: String,
    pub submitted_at: i64,
    /// 稳定码：waiting / accepted / rejected / withdrawn
    pub result: String,
    pub cooldown_until: Option<i64>,
    pub notes: String,
}

impl From<Submission> for SubmissionDto {
    fn from(s: Submission) -> Self {
        Self {
            id: s.id,
            venue: s.venue,
            submitted_at: s.submitted_at,
            result: s.result.as_str().to_string(),
            cooldown_until: s.cooldown_until,
            notes: s.notes,
        }
    }
}

/// 看板的一张卡片。
#[derive(Debug, Serialize)]
pub struct ManuscriptRowDto {
    pub id: i64,
    pub kind: String,
    pub title: String,
    /// 七态稳定码：draft / finished / ready / submitted / accepted / rejected / published
    pub status: String,
    pub word_count: i64,
    pub char_count: i64,
    /// 目标字数（null = 没设过）——「未完成散篇」比的就是它
    pub target_words: Option<i64>,
    /// 投过几家（软删的不算）
    pub submissions: i64,
    /// 还在等回音的那条：投给谁 / 何时 / 冷冻期到哪天（没有就是 null）
    pub active_venue: Option<String>,
    pub active_submitted_at: Option<i64>,
    pub active_cooldown_until: Option<i64>,
    /// 最近投给谁（含已出结果的）
    pub last_venue: Option<String>,
    pub updated_at: i64,
}

fn to_row_dto(row: ManuscriptRow) -> ManuscriptRowDto {
    let active = row.active.as_ref();
    ManuscriptRowDto {
        id: row.work.id,
        kind: row.work.kind.as_str().to_string(),
        title: row.work.title,
        status: row.work.status.as_str().to_string(),
        word_count: row.word_count,
        char_count: row.char_count,
        target_words: row.work.target_words,
        submissions: row.submissions,
        active_venue: active.map(|s| s.venue.clone()),
        active_submitted_at: active.map(|s| s.submitted_at),
        active_cooldown_until: active.and_then(|s| s.cooldown_until),
        last_venue: row.last_venue,
        updated_at: row.work.updated_at,
    }
}

/// 一篇稿子的详情：现在到哪一步 + 全部投稿记录（新的在前）。
#[derive(Debug, Serialize)]
pub struct ManuscriptDetailDto {
    pub work_id: i64,
    pub status: String,
    pub submissions: Vec<SubmissionDto>,
}

fn detail(store: &Store, work_id: i64) -> yanmo_core::Result<ManuscriptDetailDto> {
    Ok(ManuscriptDetailDto {
        work_id,
        status: store.manuscript_status(work_id)?.as_str().to_string(),
        submissions: store.submissions(work_id)?.into_iter().map(SubmissionDto::from).collect(),
    })
}

/// 记一次投稿**之前**的判词（界面拿它说明后果；`clear` = 没有拦的理由）。
#[derive(Debug, Serialize)]
pub struct SubmitVerdictDto {
    /// clear / awaiting_reply / venue_cooldown / already_tried
    pub code: &'static str,
    /// 判词针对的那一家（clear 时是 null）
    pub venue: Option<String>,
    /// 相关时间点：还在等=那次投稿的时刻；这家投过=那一次的时刻
    pub at: Option<i64>,
    /// 冷冻期到哪天（还在等 / 同刊冷冻期未满时给）
    pub cooldown_until: Option<i64>,
}

impl From<SubmitVerdict> for SubmitVerdictDto {
    fn from(v: SubmitVerdict) -> Self {
        let (code, venue, at, until) = match v {
            SubmitVerdict::Clear => ("clear", None, None, None),
            SubmitVerdict::AwaitingReply { venue, submitted_at, cooldown_until } => {
                ("awaiting_reply", Some(venue), Some(submitted_at), cooldown_until)
            }
            SubmitVerdict::VenueCooldown { venue, until } => {
                ("venue_cooldown", Some(venue), None, Some(until))
            }
            SubmitVerdict::AlreadyTried { venue, at } => ("already_tried", Some(venue), Some(at), None),
        };
        Self { code, venue, at, cooldown_until: until }
    }
}

/// 记一次投稿的回执。
#[derive(Debug, Serialize)]
pub struct ManuscriptSubmitDto {
    /// 记之前的状态（界面用来说"从草稿到已投"）
    pub from: String,
    /// 记之前算出的判词（不是 clear 时界面本该先说明后果）
    pub verdict: SubmitVerdictDto,
    /// **回读**：这篇现在的状态与全部投稿记录
    pub detail: ManuscriptDetailDto,
}

/// 看板一屏：卡片 + 界面填表要用的那个默认值。
#[derive(Debug, Serialize)]
pub struct ManuscriptBoardDto {
    /// 没写期限时界面预填几天（核心的默认冷冻期；**只是预填**，核心不拿它判）
    pub default_cooldown_days: i64,
    pub rows: Vec<ManuscriptRowDto>,
}

/// 看板：每篇没进回收站的稿子一行。
#[tauri::command]
pub fn manuscript_board(data: State<'_, AppData>) -> Result<ManuscriptBoardDto, ApiError> {
    data.with_store(|store: &mut Store| {
        Ok(ManuscriptBoardDto {
            default_cooldown_days: DEFAULT_COOLDOWN_DAYS,
            rows: store.manuscript_board()?.into_iter().map(to_row_dto).collect(),
        })
    })
}

/// 一篇稿子的详情（状态 + 全部投稿记录）。
#[tauri::command(rename_all = "snake_case")]
pub fn manuscript_detail(
    data: State<'_, AppData>,
    work_id: i64,
) -> Result<ManuscriptDetailDto, ApiError> {
    data.with_store(|store: &mut Store| detail(store, work_id))
}

/// 记一次投稿之前先问一句：会不会撞上一稿多投 / 同刊冷冻期 / 重复投。
///
/// 只读、只说明——它不是门禁：作者看过之后仍然可以投。
#[tauri::command(rename_all = "snake_case")]
pub fn manuscript_verdict(
    data: State<'_, AppData>,
    work_id: i64,
    venue: String,
) -> Result<SubmitVerdictDto, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.submission_verdict_for(work_id, &venue)?.into()))
}

/// 记一次投稿：写历史 + 把状态推进到「已投」（核心在同一个事务里做）。
#[tauri::command(rename_all = "snake_case")]
pub fn manuscript_submit(
    data: State<'_, AppData>,
    work_id: i64,
    venue: String,
    submitted_at: i64,
    cooldown_until: Option<i64>,
    notes: String,
) -> Result<ManuscriptSubmitDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let receipt =
            store.record_submission(work_id, &venue, submitted_at, cooldown_until, &notes)?;
        Ok(ManuscriptSubmitDto {
            from: receipt.from.as_str().to_string(),
            verdict: receipt.verdict.into(),
            detail: detail(store, work_id)?,
        })
    })
}

/// 改状态（看板拖拽走这条）：`to` 传七态稳定码；写完回读那一篇。
#[tauri::command(rename_all = "snake_case")]
pub fn manuscript_move(
    data: State<'_, AppData>,
    work_id: i64,
    to: String,
) -> Result<ManuscriptDetailDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.move_manuscript(work_id, ManuscriptState::parse(&to)?, "author")?;
        detail(store, work_id)
    })
}

/// 改一条投稿记录（记错目标 / 补期限 / 添备注）；回读它所属那一篇。
#[tauri::command(rename_all = "snake_case")]
pub fn manuscript_update_submission(
    data: State<'_, AppData>,
    submission_id: i64,
    venue: String,
    submitted_at: i64,
    cooldown_until: Option<i64>,
    notes: String,
) -> Result<ManuscriptDetailDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let work_id =
            store.update_submission(submission_id, &venue, submitted_at, cooldown_until, &notes)?;
        detail(store, work_id)
    })
}

/// 软删一条投稿记录（记错了）；回读它所属那一篇。
#[tauri::command(rename_all = "snake_case")]
pub fn manuscript_delete_submission(
    data: State<'_, AppData>,
    submission_id: i64,
) -> Result<ManuscriptDetailDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let work_id = store.delete_submission(submission_id)?;
        detail(store, work_id)
    })
}
