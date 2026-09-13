//! 外观 / 写作行为偏好命令：**全局打底 + 每书可选覆盖**的读写。
//!
//! 规则全在核心的 `store::appearance`（稀疏、只存改过的项、坏记录当没设过）：
//! 这里只做参数转换 + 把"读出来给人用的那份"递回去。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{Appearance, ResolvedAppearance, Store};

/// 读出来给人用的偏好（每一项都已经落到具体值）。
#[derive(Debug, Serialize)]
pub struct AppearanceDto {
    pub jump_to_end_on_latest: bool,
    /// 作者选过的字数口径（`chars` / `chars_no_punct` / `words`）；null = 没选过
    pub word_count_caliber: Option<String>,
    /// 引号用哪一套（`curly` / `corner`）：排版清理按它统一引号
    pub quote_style: String,
}

impl From<ResolvedAppearance> for AppearanceDto {
    fn from(value: ResolvedAppearance) -> Self {
        Self {
            jump_to_end_on_latest: value.jump_to_end_on_latest,
            word_count_caliber: value.word_count_caliber.map(|c| c.as_str().to_string()),
            quote_style: value.quote_style.as_str().to_string(),
        }
    }
}

/// 要改的项：**只写传进来的**，没传的保持原样。
#[derive(Debug, Deserialize)]
pub struct AppearancePatch {
    pub jump_to_end_on_latest: Option<bool>,
    pub word_count_caliber: Option<String>,
    pub quote_style: Option<String>,
}

impl From<AppearancePatch> for Appearance {
    fn from(patch: AppearancePatch) -> Self {
        Self {
            jump_to_end_on_latest: patch.jump_to_end_on_latest,
            word_count_caliber: patch.word_count_caliber,
            quote_style: patch.quote_style,
        }
    }
}

/// 读偏好（`work_id` 给 null 就是只看全局；给某本书会先看它有没有单独设过）。
#[tauri::command(rename_all = "snake_case")]
pub fn appearance_read(
    data: State<'_, AppData>,
    work_id: Option<i64>,
) -> Result<AppearanceDto, ApiError> {
    crate::acceptance::note_command("appearance_read");
    data.with_store(|store: &mut Store| Ok(store.appearance(work_id)?.into()))
}

/// 改偏好：稀疏合并，写完**回读一次**（界面显示的永远是库里那份）。
#[tauri::command(rename_all = "snake_case")]
pub fn appearance_write(
    data: State<'_, AppData>,
    work_id: Option<i64>,
    patch: AppearancePatch,
) -> Result<AppearanceDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_appearance(work_id, &patch.into())?;
        Ok(store.appearance(work_id)?.into())
    })
}

/// 回到默认（书的覆盖则是"回到继承全局"）。
#[tauri::command(rename_all = "snake_case")]
pub fn appearance_reset(
    data: State<'_, AppData>,
    work_id: Option<i64>,
) -> Result<AppearanceDto, ApiError> {
    data.with_store(|store: &mut Store| {
        store.reset_appearance(work_id)?;
        Ok(store.appearance(work_id)?.into())
    })
}
