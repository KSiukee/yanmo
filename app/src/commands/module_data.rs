//! 模块数据出口：体检 / 隔离 / 解除隔离 / 导出（一致性快照）与清除（显式二次确认之后）。

use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;
use serde::Serialize;
use yanmo_core::store::{ModuleAudit, ModuleData, ModuleMirror, ModuleMirrorCheck, ModuleRebuild};

use super::module_run::{status_module, stop_module, ModuleRuns};
use crate::error::ApiError;
use crate::storage::AppData;

#[derive(Debug, Clone, Serialize)]
pub struct ModuleDataStatsView {
    pub tables: usize,
    pub rows: i64,
    pub db_bytes: u64,
    pub degraded: bool,
    pub rejected: u64,
    pub isolated: bool,
    pub isolation_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleIsolateReceipt {
    pub isolated: bool,
    /// 隔离前那份镜像留成了没有（没留成也照样隔离——数据本身不会因为隔离而消失）。
    pub mirrored: bool,
    pub mirror_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleMirrorRefresh {
    /// 这次写下的镜像。
    pub written: ModuleMirror,
    /// 写之前的旧镜像与主库的核对结果（以前没写过 = `None`）。
    pub before: Option<ModuleMirrorCheck>,
    /// 旧镜像读不出来时的原因（这时 [`Self::before`] 是 `None`）。
    pub before_error: Option<String>,
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
    let isolation = module_data.isolation(&module_id)?;
    Ok(ModuleDataStatsView {
        tables: stats.tables,
        rows: stats.rows,
        db_bytes: stats.db_bytes,
        degraded: health.degraded,
        rejected: health.rejected,
        isolated: isolation.is_some(),
        isolation_reason: isolation.map(|state| state.reason).unwrap_or_default(),
    })
}

/// 体检一个模块的数据：**只读**，给事实与一句结论（不做健康评分）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_data_audit(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ModuleAudit, ApiError> {
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    Ok(module_data.audit(&module_id)?)
}

/// 隔离一个模块的数据：先尽力停掉它、**先留一份镜像**，再把「隔离」写进 `modules.db`。
///
/// 停下只是礼貌——**真正拦住读写的是宿主每次调用都读的那一行**，所以就算停不下来，
/// 隔离一旦写进去，模块也拿不到数据。镜像同理：写不成也照样隔离（隔离不删数据），
/// 只是把「没留成」如实带回界面。
#[tauri::command(rename_all = "snake_case")]
pub fn module_data_isolate(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
    reason: String,
) -> Result<ModuleIsolateReceipt, ApiError> {
    if status_module(runs.inner(), &module_id)?.running {
        stop_module(runs.inner(), &module_id)?;
    }
    let mut module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    let mirror_error = module_data.mirror(&module_id).err().map(|error| error.to_string());
    module_data.isolate(&module_id, &reason)?;
    Ok(ModuleIsolateReceipt { isolated: true, mirrored: mirror_error.is_none(), mirror_error })
}

/// 解除隔离（**只认作者点这一下**；不自动恢复）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_data_release(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<bool, ApiError> {
    let mut module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    Ok(module_data.release_isolation(&module_id)?)
}

/// 写一份镜像，并把**写之前的旧镜像**跟主库的核对结果一并带回。
///
/// 旧镜像读不出来不算失败（恰恰是最该重写它的时候）——把它记成 `before_error` 带回去。
#[tauri::command(rename_all = "snake_case")]
pub fn module_data_mirror(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ModuleMirrorRefresh, ApiError> {
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    let (before, before_error) = match module_data.check_mirror(&module_id) {
        Ok(check) if check.state == "missing" => (None, None),
        Ok(check) => (Some(check), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let written = module_data.mirror(&module_id)?;
    Ok(ModuleMirrorRefresh { written, before, before_error })
}

/// 用镜像重建这一格的数据（**必须先隔离**，否则会覆盖正在用的数据）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_data_rebuild(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ModuleRebuild, ApiError> {
    let mut module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    Ok(module_data.rebuild_from_mirror(&module_id)?)
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
