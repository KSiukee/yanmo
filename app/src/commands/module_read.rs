//! 读取放行（渐进放行）的界面出口：**列出等作者回答的事 / 作答**。
//!
//! 闸门在启动器那侧（模块读正文时按档位限量）；这里只做两件事：把"它在等你回答"
//! 摆到界面上，以及把作者的选择写回去。规则全在核心（`module_read_consent`）。

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;
use yanmo_core::store::{recent_entries, ModuleData, ReadAnswer, Store, READ_LOG_FILE};

use crate::error::ApiError;
use crate::storage::AppData;

/// 一件等作者回答的事。
#[derive(Debug, Clone, Serialize)]
pub struct ReadAskView {
    pub module_id: String,
    /// `first`（只告知）/ `second`（问要不要放宽到两篇）/ `ultimate`（第三次：不再干预）。
    pub kind: String,
    /// 这是第几次。
    pub refusals: u64,
    /// 最近一次被拦下时它正在读的那本书（书名；查不到就是 `None`）。
    pub last_work: Option<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_read_asks(data: State<'_, AppData>) -> Result<Vec<ReadAskView>, ApiError> {
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    let asks = module_data.read_asks()?;
    if asks.is_empty() {
        return Ok(Vec::new());
    }
    let entries = recent_entries(&data.data_dir().join(READ_LOG_FILE), 5000);
    let titles: HashMap<i64, String> = data
        .with_store(|store: &mut Store| store.shelf())?
        .into_iter()
        .map(|entry| (entry.work.id, entry.work.title))
        .collect();
    Ok(asks
        .into_iter()
        .map(|(module_id, ask)| {
            // 最近一次被拦下的那条读账里记着"正在读哪本书"
            let last_work = entries
                .iter()
                .rev()
                .find(|entry| entry.module == module_id && !entry.allowed())
                .and_then(|entry| entry.work)
                .and_then(|work| titles.get(&work).cloned());
            ReadAskView {
                module_id,
                kind: ask.kind.to_string(),
                refusals: ask.refusals,
                last_work,
            }
        })
        .collect())
}

/// 作者的作答（`acknowledge` / `allow_more` / `keep_limited`）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_read_answer(
    data: State<'_, AppData>,
    module_id: String,
    answer: String,
) -> Result<bool, ApiError> {
    let parsed = ReadAnswer::parse(&answer).ok_or_else(|| {
        ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::MODULE_READ_ANSWER_INVALID,
            [("value", answer.clone())],
        ))
    })?;
    let mut module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    module_data.answer_read_ask(&module_id, parsed)?;
    Ok(true)
}
