//! 管理页的**健康读数**：每个模块一行——上次跑成什么样、是不是在退避、数据有没有被隔离。
//!
//! 这些事实早就有了（留痕 / 退避账 / 隔离表），这一条只负责**摆到列表上**：
//! 作者不该为了看"它最近还好吗"逐张卡点进事实页。

use serde::Serialize;
use tauri::State;
use yanmo_core::store::{ModuleData, Store};

use crate::error::ApiError;
use crate::storage::AppData;

#[derive(Debug, Clone, Serialize)]
pub struct ModuleHealthView {
    pub module_id: String,
    /// 上次运行的时刻（没跑过 = 0）
    pub last_at: i64,
    /// `ok` / `error` / `timeout` / `unfinished` / `cut`；没跑过 = 空串
    pub last_outcome: String,
    pub last_code: String,
    /// 连续失败次数（退避账里的）
    pub failures: i64,
    /// 自动唤起要等到什么时候（0 = 没在退避）
    pub backoff_until: i64,
    /// 数据被隔离了（读、写、迁移全停）
    pub isolated: bool,
}

/// 一个模块的退避参数（界面读它显示/编辑）。
#[derive(Debug, Clone, Serialize)]
pub struct RetryView {
    pub module_id: String,
    pub base_ms: i64,
    pub max_ms: i64,
    pub enabled: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_retry_get(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<RetryView, ApiError> {
    let module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let policy = module_data.retry_policy(&module_id)?;
    Ok(RetryView {
        module_id,
        base_ms: policy.base_ms,
        max_ms: policy.max_ms,
        enabled: policy.enabled,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_retry_set(
    data: State<'_, AppData>,
    module_id: String,
    base_ms: i64,
    max_ms: i64,
    enabled: bool,
) -> Result<RetryView, ApiError> {
    let mut module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let policy = yanmo_core::store::RetryPolicy { base_ms, max_ms, enabled };
    module_data.set_retry_policy(&module_id, &policy)?;
    Ok(RetryView { module_id, base_ms, max_ms, enabled })
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_retry_clear(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<(), ApiError> {
    let mut module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    module_data.clear_retry_policy(&module_id)?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_health_list(
    data: State<'_, AppData>,
) -> Result<Vec<ModuleHealthView>, ApiError> {
    let module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let ids = data.with_store(|store: &mut Store| {
        Ok(store
            .module_cards()?
            .into_iter()
            .map(|card| card.id)
            .collect::<Vec<_>>())
    })?;
    let mut out = Vec::with_capacity(ids.len());
    for module_id in ids {
        let (last_at, last_outcome, last_code) = module_data
            .recent_runs(&module_id, 1)?
            .into_iter()
            .next()
            .map(|run| (run.started_at, run.outcome, run.code))
            .unwrap_or((0, String::new(), String::new()));
        let backoff = module_data.backoff(&module_id)?;
        let isolated = module_data.isolation(&module_id)?.is_some();
        out.push(ModuleHealthView {
            module_id,
            last_at,
            last_outcome,
            last_code,
            failures: backoff.as_ref().map(|item| item.failures).unwrap_or(0),
            backoff_until: backoff.map(|item| item.next_at).unwrap_or(0),
            isolated,
        });
    }
    Ok(out)
}
