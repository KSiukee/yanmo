//! 模块管理页命令域：**列卡片 + 激活/冻结 + 收藏/置顶 + 卸载（保留数据）**。
//!
//! 纪律与其它命令域一致：只做参数转换，规则全在核心；**界面不传路径、不碰文件**。
//! 启动/停止在 `module_run` 命令域；这里只管注册表与导入。

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::State;

use super::module_run::ModuleRuns;
use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{
    CertificationSet, ImportPreview, ModuleCard, Store,
};
use yanmo_proto::capability::ModuleCapabilities;

/// 界面选好、等着确认的模块文件夹。**路径只活在壳里**：界面拿不到，也递不进来。
#[derive(Default)]
pub struct ImportStage {
    staged: Mutex<Option<StagedImport>>,
}

struct StagedImport {
    dir: PathBuf,
    module_id: String,
    fingerprint: String,
}

impl ImportStage {
    fn clear(&self) {
        if let Ok(mut guard) = self.staged.lock() {
            *guard = None;
        }
    }

    fn set(&self, dir: PathBuf, module_id: String, fingerprint: String) -> Result<(), ApiError> {
        let mut guard = self.staged.lock().map_err(|_| ApiError::new("shell.store_unavailable"))?;
        *guard = Some(StagedImport { dir, module_id, fingerprint });
        Ok(())
    }

    fn take(&self) -> Result<StagedImport, ApiError> {
        let mut guard = self.staged.lock().map_err(|_| ApiError::new("shell.store_unavailable"))?;
        guard.take().ok_or_else(|| ApiError::new("shell.import_not_staged"))
    }
}

/// 管理页列表（置顶 → 收藏 → 安装顺序）。
#[tauri::command]
pub fn modules_list(data: State<'_, AppData>) -> Result<Vec<ModuleCard>, ApiError> {
    data.with_store(|store: &mut Store| store.module_cards())
}

/// 激活 / 冻结。冻结只改策略与可调用性，**不删数据、不卸载**。
#[tauri::command(rename_all = "snake_case")]
pub fn module_set_enabled(
    data: State<'_, AppData>,
    module_id: String,
    enabled: bool,
) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_module_enabled(&module_id, enabled)?;
        Ok(())
    })
}

/// 收藏（只影响管理页排序）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_set_favorite(
    data: State<'_, AppData>,
    module_id: String,
    favorite: bool,
) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| store.set_module_favorite(&module_id, favorite))
}

/// 置顶（只影响管理页排序）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_set_pinned(
    data: State<'_, AppData>,
    module_id: String,
    pinned: bool,
) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| store.set_module_pinned(&module_id, pinned))
}

/// 后台运行偏好：离开管理页时是否允许继续跑（**不是自启**）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_set_background(
    data: State<'_, AppData>,
    module_id: String,
    background: bool,
) -> Result<ModuleCard, ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_module_background(&module_id, background)?;
        store.module_card(&module_id)
    })
}

/// 全局离线开关：真时任何模块都不该拿到出网 import；切完停平台，下次启动生效。
#[tauri::command(rename_all = "snake_case")]
pub fn network_offline(data: State<'_, AppData>) -> Result<bool, ApiError> {
    data.with_store(|store: &mut Store| store.network_offline())
}

#[tauri::command(rename_all = "snake_case")]
pub fn network_set_offline(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    offline: bool,
) -> Result<bool, ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_network_offline(offline)?;
        Ok(offline)
    })?;
    runs.stop_platform()?;
    Ok(offline)
}

/// 全局域名白名单：空 = 不额外限制；写入后停平台，下次启动生效。
#[tauri::command(rename_all = "snake_case")]
pub fn network_allowlist(data: State<'_, AppData>) -> Result<Vec<String>, ApiError> {
    data.with_store(|store: &mut Store| store.network_allowlist())
}

#[tauri::command(rename_all = "snake_case")]
pub fn network_set_allowlist(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    hosts: Vec<String>,
) -> Result<Vec<String>, ApiError> {
    let clean = data.with_store(|store: &mut Store| {
        store.set_network_allowlist(&hosts)?;
        store.network_allowlist()
    })?;
    runs.stop_platform()?;
    Ok(clean)
}

/// 模块级联网开关：改授予，清单没声明的一律拒；切完停平台，下次启动生效。
#[tauri::command(rename_all = "snake_case")]
pub fn module_set_network(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
    network: bool,
) -> Result<ModuleCard, ApiError> {
    let card = data.with_store(|store: &mut Store| {
        store.set_module_network(&module_id, network)?;
        store.module_card(&module_id)
    })?;
    runs.stop_platform()?;
    Ok(card)
}

/// 卸载：**只移除注册表，模块数据默认保留**（删除数据是另一个显式动作）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_uninstall(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<(), ApiError> {
    runs.stop_platform()?;
    let credentials = data
        .with_store(|store: &mut Store| store.module_credentials(&module_id))
        .unwrap_or_default();
    for binding in credentials {
        let _ = yanmo_vault::remove(&module_id, &binding.name);
    }
    data.with_store(|store: &mut Store| store.uninstall_module(&module_id))
}

/// 选一个模块文件夹并预览（**不安装**）。取消返回 `None`。
///
/// 路径留在壳里（[`ImportStage`]）；界面只拿到事实：清单、声明的权限、指纹、协议。
#[tauri::command(rename_all = "snake_case")]
pub fn module_import_pick(
    stage: State<'_, ImportStage>,
    title: String,
) -> Result<Option<ImportPreview>, ApiError> {
    stage.clear();
    let Some(dir) = crate::pick_dir::pick(&title) else {
        return Ok(None);
    };
    let preview = yanmo_core::store::inspect_module_package(&dir).map_err(ApiError::from)?;
    stage.set(dir, preview.module_id.clone(), preview.fingerprint.clone())?;
    Ok(Some(preview))
}

/// 确认导入：权限由用户在预览里逐项勾选；**重新校验 + 重新算指纹**。
#[tauri::command(rename_all = "snake_case")]
pub fn module_import_confirm(
    data: State<'_, AppData>,
    stage: State<'_, ImportStage>,
    runs: State<'_, ModuleRuns>,
    granted: ModuleCapabilities,
    expected_fingerprint: String,
) -> Result<ModuleCard, ApiError> {
    let staged = stage.take()?;
    if staged.fingerprint != expected_fingerprint {
        return Err(ApiError::from(yanmo_core::Error::invalid(
            yanmo_core::error_codes::codes::MODULE_IMPORT_CHANGED,
        )));
    }
    let card = data.with_store(|store: &mut Store| {
        let managed = data.data_dir().join("modules").join(&staged.module_id);
        store.import_module_managed(
            &staged.dir,
            &managed,
            &granted,
            &expected_fingerprint,
            "folder",
            &CertificationSet::built_in(),
        )?;
        store.module_card(&staged.module_id)
    })?;
    // 平台启动时扫描模块根建 handler；新导入的模块要让下一次启动重新扫。
    runs.stop_platform()?;
    Ok(card)
}

/// 放弃这次导入（清掉壳里暂存的路径）。
#[tauri::command]
pub fn module_import_cancel(stage: State<'_, ImportStage>) -> Result<(), ApiError> {
    stage.clear();
    Ok(())
}
