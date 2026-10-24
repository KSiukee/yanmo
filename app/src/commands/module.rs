//! 模块管理页命令域：**列卡片 + 激活/冻结 + 收藏/置顶 + 卸载（保留数据）**。
//!
//! 纪律与其它命令域一致：只做参数转换，规则全在核心；**界面不传路径、不碰文件**。
//! 启动模块不在这里——宿主/启动器那一层还没接到默认壳（模块平台是可选构建）。

use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{ModuleCard, Store};

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
