//! 模块管理页命令域：**列卡片 + 激活/冻结 + 收藏/置顶 + 卸载（保留数据）**。
//!
//! 纪律与其它命令域一致：只做参数转换，规则全在核心；**界面不传路径、不碰文件**。
//! 启动模块不在这里——宿主/启动器那一层还没接到默认壳（模块平台是可选构建）。

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

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

/// 卸载：**只移除注册表，模块数据默认保留**（删除数据是另一个显式动作）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_uninstall(data: State<'_, AppData>, module_id: String) -> Result<(), ApiError> {
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
    granted: ModuleCapabilities,
    expected_fingerprint: String,
) -> Result<ModuleCard, ApiError> {
    let staged = stage.take()?;
    if staged.fingerprint != expected_fingerprint {
        return Err(ApiError::from(yanmo_core::Error::invalid(
            yanmo_core::error_codes::codes::MODULE_IMPORT_CHANGED,
        )));
    }
    data.with_store(|store: &mut Store| {
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
    })
}

/// 放弃这次导入（清掉壳里暂存的路径）。
#[tauri::command]
pub fn module_import_cancel(stage: State<'_, ImportStage>) -> Result<(), ApiError> {
    stage.clear();
    Ok(())
}

/// 跑一次模块的报告（宿主跑完的那一份）。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRunReport {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub emitted: Vec<String>,
    pub stderr: String,
    pub ipc_error: Option<String>,
    pub job_assigned: bool,
}

/// 启动（跑一次）一个**已装且受管**的模块：先核对包指纹，再拉独立启动器。
///
/// 启动器是独立进程（`yanmo-launch`），壳不链接沙箱/IPC；宿主二进制与启动器
/// 必须与壳同目录（或由 `YANMO_HOST_BIN` / `YANMO_LAUNCH_BIN` 指定）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_run(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ModuleRunReport, ApiError> {
    let record = data.with_store(|store: &mut Store| store.module_record(&module_id))?;
    if !record.is_active() {
        return Err(ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::MODULE_DISABLED,
            [("module_id", module_id.clone())],
        )));
    }
    let managed = data.data_dir().join("modules").join(&module_id);
    data.with_store(|store: &mut Store| store.verify_module_package(&module_id, &managed))?;

    let host = resolve_bin("yanmo-host", "YANMO_HOST_BIN")?;
    let launcher = resolve_bin("yanmo-launch", "YANMO_LAUNCH_BIN")?;
    let mut command = Command::new(&launcher);
    command
        .arg("--db")
        .arg(data.db_path())
        .arg("--module")
        .arg(&managed)
        .arg("--host")
        .arg(&host)
        .arg("--timeout-secs")
        .arg("60")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().map_err(|error| {
        ApiError::new("shell.module_launch_failed").caused_by(error)
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ApiError::new("shell.module_launch_failed").caused_by(detail));
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        ApiError::new("shell.module_launch_failed").caused_by(error)
    })?;
    Ok(ModuleRunReport {
        exit_code: value
            .get("exit_code")
            .and_then(|value| value.as_i64())
            .map(|code| code as i32),
        timed_out: value.get("timed_out").and_then(|value| value.as_bool()).unwrap_or(false),
        emitted: value
            .get("emitted")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        stderr: value
            .get("stderr")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        ipc_error: value
            .get("ipc_error")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        job_assigned: value.get("job_assigned").and_then(|value| value.as_bool()).unwrap_or(false),
    })
}

fn resolve_bin(name: &str, env_name: &str) -> Result<PathBuf, ApiError> {
    if let Ok(path) = std::env::var(env_name) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()
        .map_err(|_| ApiError::new("shell.module_platform_unavailable"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| ApiError::new("shell.module_platform_unavailable"))?;
    let candidates = if cfg!(windows) {
        vec![dir.join(format!("{name}.exe")), dir.join(name)]
    } else {
        vec![dir.join(name)]
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| ApiError::new("shell.module_platform_unavailable"))
}
