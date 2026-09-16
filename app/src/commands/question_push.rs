//! 叩问的「推」命令：**过了门槛才开口，开口就算问过**。
//!
//! 与 [`super::question`]（面板那一族：挑什么问、怎么处置、答案往哪落）分开的原因：
//! 这一族问的是**时机与配额**（软件自己什么时候凑上来），那一族问的是**内容与去处**。
//! 两者的变化理由不一样，混在一个文件里只会让那一份越来越难改。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::question::PushQuota;
use yanmo_core::store::{PushOutcome, SelectedQuestion};

/// 主动问一句（推）：过了门槛才开口，开口就算"问过"。
///
/// 门槛（每天几次 / 冷却多久）由界面把**当前生效的偏好**传进来——核心按它算，
/// 并**在同一笔里**把卡迁到「已问」+ 记下今天的账（见核心 `store::question_push`）。
/// `today` 是作者本地那一天（`YYYYMMDD`），由界面按本地时区算好；`reason` 是
/// 为什么现在开口（`new_chapter` / `idle` / `chapter_done`），它进 op-log 的 trigger。
///
/// 返回 `code`：`push.asked`（问出去了）/ `push.quota_used` / `push.too_soon` /
/// `push.nothing_to_ask`——**错过一次推是静默的**，界面只在 `push.asked` 时冒头。
#[tauri::command(rename_all = "snake_case")]
pub fn question_push(
    data: State<'_, AppData>,
    work_id: i64,
    node_id: Option<i64>,
    per_day: i64,
    cooldown_minutes: i64,
    today: i64,
    reason: String,
) -> Result<PushDoneDto, ApiError> {
    crate::acceptance::note_command("question_push");
    data.with_store(|store| {
        let outcome = store.push_question(
            work_id,
            node_id,
            PushQuota { per_day, cooldown_minutes },
            today,
            &reason,
        )?;
        let code = outcome.as_str().to_string();
        let question = match outcome {
            PushOutcome::Pushed { question } => Some(question),
            _ => None,
        };
        Ok(PushDoneDto { code, question })
    })
}

/// 推一次的结果：给界面那张卡（问出去了才有）+ 一个码（为什么没问）。
#[derive(Debug, Serialize)]
pub struct PushDoneDto {
    pub code: String,
    pub question: Option<SelectedQuestion>,
}

