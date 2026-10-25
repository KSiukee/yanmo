//! 打开模块的子 WebView：只加载壳起的只读静态服务，不挂 Tauri 能力。

use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use super::module_run::ModuleRuns;
use crate::error::ApiError;
use crate::module_ui::ModuleUiState;
use crate::storage::AppData;
use yanmo_core::store::Store;

#[tauri::command(rename_all = "snake_case")]
pub fn module_ui_open(
    app: AppHandle,
    data: State<'_, AppData>,
    ui: State<'_, ModuleUiState>,
    _runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<(), ApiError> {
    let record = data.with_store(|store: &mut Store| store.module_record(&module_id))?;
    if !record.is_active() {
        return Err(ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::MODULE_DISABLED,
            [("module_id", module_id.clone())],
        )));
    }
    let managed = data.data_dir().join("modules").join(&module_id);
    data.with_store(|store: &mut Store| store.verify_module_package(&module_id, &managed))?;
    let root = managed.join("ui");
    if !root.join("index.html").is_file() {
        return Err(ApiError::new("shell.module_ui_missing"));
    }

    // 换模块：先关旧窗口/旧服务，避免两个服务同时占着。
    if let Some(window) = app.get_webview_window("module-ui") {
        let _ = window.destroy();
    }
    ui.clear();

    ui.set(root);
    let parsed = "moduleui://localhost/index.html"
        .parse()
        .map_err(|error| ApiError::new("shell.module_ui_failed").caused_by(error))?;
    let window = WebviewWindowBuilder::new(&app, "module-ui", WebviewUrl::CustomProtocol(parsed))
        .title(module_id.clone())
        .inner_size(960.0, 720.0)
        .min_inner_size(480.0, 360.0)
        .build()
        .map_err(|error| ApiError::new("shell.module_ui_failed").caused_by(error))?;
    let shared = ui.root.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Ok(mut slot) = shared.lock() {
                *slot = None;
            }
        }
    });
    Ok(())
}
