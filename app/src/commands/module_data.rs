//! 模块数据出口：导出（一致性快照）与清除（显式二次确认之后）。

use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;
use serde::Serialize;
use yanmo_core::store::ModuleData;

use super::module_run::ModuleRuns;
use crate::error::ApiError;
use crate::storage::AppData;

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDataStatsView {
    pub tables: usize,
    pub rows: i64,
    pub db_bytes: u64,
    pub degraded: bool,
    pub rejected: u64,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_data_export(
    data: State<'_, AppData>,
    module_id: String,
    title: String,
) -> Result<bool, ApiError> {
    let Some(dir) = crate::pick_dir::pick(&title) else {
        return Ok(false);
    };
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    let safe: String = module_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' { ch } else { '_' })
        .collect();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    module_data.export_namespace(&module_id, dir.join(format!("modules-{safe}-{stamp}.db")))?;
    Ok(true)
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_data_stats(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ModuleDataStatsView, ApiError> {
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    let stats = module_data.stats(&module_id)?;
    let health = module_data.health(&module_id)?;
    Ok(ModuleDataStatsView {
        tables: stats.tables,
        rows: stats.rows,
        db_bytes: stats.db_bytes,
        degraded: health.degraded,
        rejected: health.rejected,
    })
}

/// 清除一个模块的全部数据。**先停整个平台**，避免子进程还在写时拆表。
#[tauri::command(rename_all = "snake_case")]
pub fn module_data_clear(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<usize, ApiError> {
    runs.stop_platform()?;
    let mut module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    Ok(module_data.drop_namespace(&module_id)?)
}
