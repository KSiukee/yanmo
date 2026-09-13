//! 「稿子放在哪」的命令域：首启引导、选新位置、真正搬家。
//!
//! # 纪律（这条是本模块最要紧的事）
//!
//! **命令层不收路径，也不回路径当"通行证"**：选目录的对话框在壳里开，
//! 选中的路径**只记在壳里**（[`AppData::set_pending_dir`]），界面随后只说一句"搬吧"。
//! 这样界面既无从指定别处、也没有"递一个路径进来"的入口——
//! 数据目录的权威始终只有一处（壳），这正是 `storage` 那条链路要守住的东西。
//!
//! 编排放这里：close → 复制 → 核对 → 记记录 → 重启；每一步的实现在核心与 [`AppData`]。

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::location::Risk;

/// 现在稿子放哪、要不要引导、推荐放哪（都是**只读报告**）。
#[derive(Debug, Serialize)]
pub struct LocationInfo {
    /// 当前数据目录
    pub path: String,
    /// 位置记录文件（作者想知道"你凭什么记得"时可以看它）
    pub pointer: String,
    /// 第一次用：界面该弹首启引导
    pub first_run: bool,
    /// 推荐位置（首启时与 `path` 相同）
    pub suggested_path: Option<String>,
    /// 推荐它的理由码（界面对字典翻人话）
    pub suggested_reason: Option<String>,
}

/// 界面刚选好的位置：落在哪、有什么风险、那儿现在有多少东西。
#[derive(Debug, Serialize)]
pub struct PickedDir {
    pub path: String,
    /// 风险码清单（synced / desktop / drive_root / inside_data / occupied / removable）
    pub risks: Vec<String>,
    /// 这个目录里现有的条目数（"这里不是空的"要让人看见）
    pub entries: usize,
}

/// 搬完的账（旧位置**一字不删**）。
#[derive(Debug, Serialize)]
pub struct RelocationDto {
    pub path: String,
    pub files: usize,
    pub bytes: u64,
}

/// 现在的情况。
#[tauri::command(rename_all = "snake_case")]
pub fn data_location_info(data: State<'_, AppData>) -> LocationInfo {
    let suggestion = data.suggestion();
    LocationInfo {
        path: data.data_dir().to_string_lossy().to_string(),
        pointer: data.pointer().to_string_lossy().to_string(),
        first_run: data.is_first_run(),
        suggested_path: suggestion.map(|item| item.dir.to_string_lossy().to_string()),
        suggested_reason: suggestion.map(|item| item.reason.code().to_string()),
    }
}

/// 首启选「就用这里」：把当前目录记下来，之后不再弹引导。
#[tauri::command(rename_all = "snake_case")]
pub fn data_location_confirm(data: State<'_, AppData>) -> Result<(), ApiError> {
    data.confirm_location()
}

/// 让作者挑一个文件夹（窗口标题由界面传进来——**壳不产界面文案**）。
///
/// 选中的路径**记在壳里**，界面只拿到"选到哪了 + 那里有什么坑"。
#[tauri::command(rename_all = "snake_case")]
pub fn data_location_pick(data: State<'_, AppData>, title: String) -> Option<PickedDir> {
    let dir = crate::pick_dir::pick(&title)?;
    let mut risks: Vec<String> = yanmo_core::location::risks(&dir, data.sources(), &data.data_dir())
        .iter()
        .map(|risk| risk.code().to_string())
        .collect();
    // 可移动盘 / 网络盘要壳问系统（核心只做纯路径判定）
    if crate::volume::is_removable(&dir) {
        risks.push(Risk::Removable.code().to_string());
    }
    let entries = crate::pick_dir::entry_count(&dir);
    data.set_pending_dir(dir.clone());
    Some(PickedDir { path: dir.to_string_lossy().to_string(), risks, entries })
}

/// 搬吧：复制 → 核对 → 记下新位置，然后把壳重启（与换库同一条重启路）。
#[tauri::command(rename_all = "snake_case")]
pub fn data_location_move(
    app: AppHandle,
    data: State<'_, AppData>,
) -> Result<RelocationDto, ApiError> {
    let target = data
        .take_pending_dir()
        .ok_or_else(|| ApiError::new("shell.relocate_pending_missing"))?;
    let report = data.relocate(&target)?;
    // 单实例锁先**收出来**、在重启前松开：新进程启动时回来要同一把锁，
    // 旧进程不放，新进程就只会看见"已经在运行"而退出——软件就再也回不来了。
    let instance = data.take_instance_lock();
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1200));
        drop(instance);
        handle.restart();
    });
    Ok(RelocationDto {
        path: report.dir.to_string_lossy().to_string(),
        files: report.files,
        bytes: report.bytes,
    })
}

/// 算了，不搬了：把待定的路径扔掉（壳里不留悬着的选择）。
#[tauri::command(rename_all = "snake_case")]
pub fn data_location_cancel(data: State<'_, AppData>) {
    let _ = data.take_pending_dir();
}
