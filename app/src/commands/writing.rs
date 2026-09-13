//! 码字统计命令域：**今日进度**与**码字日历**的只读入口。
//!
//! 分工：账本、口径、日期（哪一天）全在核心的 `store::writing`；这里只做两件事——
//! 把界面递进来的时区偏移转给核心、把结果原样包成 JSON。
//!
//! 为什么时区要从界面来：核心不猜时区（不引时区库），能拿到的是界面的
//! `-new Date().getTimezoneOffset()`——与备份那条线同一套口径。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{Store, WritingDay};

/// 一天的字数（三个口径都给，界面按当前口径显示）。
#[derive(Debug, Serialize)]
pub struct WritingDayDto {
    pub day: String,
    pub chars: i64,
    pub chars_no_punct: i64,
    pub words: i64,
}

impl From<WritingDay> for WritingDayDto {
    fn from(value: WritingDay) -> Self {
        Self {
            day: value.day,
            chars: value.chars,
            chars_no_punct: value.chars_no_punct,
            words: value.words,
        }
    }
}

/// 日历面板要的全部数据：今日、区间内的每一天、连续天数。
///
/// 三样一起回，是因为它们是**同一张账本的同一次读**——界面分三次问只会让
/// 跨零点那一刻前后对不上（明明同一屏，今日数与日历里今天那格不一样）。
#[derive(Debug, Serialize)]
pub struct WritingOverviewDto {
    pub today: WritingDayDto,
    /// 区间内有记录的日子（没记录的日子不返回，界面自己补齐空格）
    pub days: Vec<WritingDayDto>,
    /// 连续码字天数（到今天为止）
    pub streak: i64,
}

/// 今日码字——状态栏那行小字用它（落盘后刷一次，开销只有一行查询）。
#[tauri::command(rename_all = "snake_case")]
pub fn writing_today(
    data: State<'_, AppData>,
    work_id: Option<i64>,
    tz_offset_minutes: i32,
) -> Result<WritingDayDto, ApiError> {
    crate::acceptance::note_command("writing_today");
    data.with_store(|store: &mut Store| {
        Ok(store.writing_today(work_id, tz_offset_minutes)?.into())
    })
}

/// 日历数据：`from_day` / `to_day` 是本地日期（`2026-09-01`），闭区间。
#[tauri::command(rename_all = "snake_case")]
pub fn writing_overview(
    data: State<'_, AppData>,
    work_id: Option<i64>,
    tz_offset_minutes: i32,
    from_day: String,
    to_day: String,
) -> Result<WritingOverviewDto, ApiError> {
    crate::acceptance::note_command("writing_overview");
    data.with_store(|store: &mut Store| {
        Ok(WritingOverviewDto {
            today: store.writing_today(work_id, tz_offset_minutes)?.into(),
            days: store
                .writing_between(work_id, &from_day, &to_day)?
                .into_iter()
                .map(WritingDayDto::from)
                .collect(),
            streak: store.writing_streak(work_id, tz_offset_minutes)?,
        })
    })
}
