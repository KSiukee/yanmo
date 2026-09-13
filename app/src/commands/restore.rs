//! 从备份恢复的命令域：列来源、体检预览、手选一个库文件、真正换库。
//!
//! # 纪律
//!
//! - **这里一行文件操作都没有**：体检、留底、回滚、换库全在核心（`store::restore`），
//!   壳只管"关库 → 叫核心干活 → 重启"这条编排；
//! - **绝不硬恢复**：执行前当场再体检一次——界面看过的那一遍不算数，
//!   这份包可能在预览之后被改坏、被拔盘；
//! - **选文件也不给界面路径能力**：系统自带的打开对话框在壳里调，界面只拿到一个字符串；
//!   连对话框的文案都由界面传进来（界面文案只该有一处出处）。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{
    scan_packages, PackageBrief, RestoreOutcome, RestorePreview, Store, KEEP_FOLDER,
};

/// 可恢复的来源现状：看得见的备份包 + 扫过的位置 + 留底目录（都是给作者看的路径）。
#[derive(Debug, Serialize)]
pub struct RestoreSourcesDto {
    pub packages: Vec<PackageBrief>,
    /// 扫过的备份位置（作者勾过、且这一刻目录真的还在的）
    pub roots: Vec<String>,
    pub data_dir: String,
    /// 换库时原库会留底到哪儿
    pub keep_dir: String,
}

/// 数据目录（库文件所在的那一层）。
fn data_dir_of(data: &AppData) -> PathBuf {
    data.db_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 列出能恢复的备份：扫作者勾过的每个备份位置。
///
/// 盘不在就扫不到——**列表本来就该只列"现在真拿得到"的东西**，
/// 列一堆插着盘才能恢复的条目只会让作者点了才发现不行。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_sources(data: State<'_, AppData>) -> Result<RestoreSourcesDto, ApiError> {
    let config = data.with_store(|store: &mut Store| Ok(store.backup_config()?))?;
    let roots: Vec<PathBuf> = config
        .targets
        .iter()
        .map(|target| PathBuf::from(&target.path))
        .filter(|path| path.is_dir())
        .collect();
    let packages = scan_packages(&roots);
    let data_dir = data_dir_of(&data);
    Ok(RestoreSourcesDto {
        packages,
        roots: roots.iter().map(|path| path.to_string_lossy().to_string()).collect(),
        keep_dir: data_dir.join(KEEP_FOLDER).to_string_lossy().to_string(),
        data_dir: data_dir.to_string_lossy().to_string(),
    })
}

/// 恢复前看一眼：这份体不体检通过、换上去会退回几天 / 少多少字。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_preview(
    data: State<'_, AppData>,
    source: String,
) -> Result<RestorePreview, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.preview_restore(Path::new(&source))?))
}

/// 真正换库：**先关库 → 留底 → 换库**，成功后壳自己重启。
///
/// 返回成功只意味着"数据目录里已经是备份那一份"——界面该把这句显示给作者，
/// 然后窗口会重启（重启由壳发起，不靠界面记得回来调）。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_apply(
    app: AppHandle,
    data: State<'_, AppData>,
    source: String,
    tz_offset_minutes: i32,
) -> Result<RestoreOutcome, ApiError> {
    let path = PathBuf::from(&source);
    // 当场再体检一次（界面上看过的那遍不算数）
    let preview = data.with_store(|store: &mut Store| Ok(store.preview_restore(&path)?))?;
    if !preview.can_restore {
        return Err(ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::BACKUP_RESTORE_BLOCKED,
            [("reason", preview.verify.problems.join("；"))],
        )));
    }
    let outcome = data.restore_apply(&path, tz_offset_minutes)?;
    // 换库成功：给界面一点时间把这句"换好了"显示出来，然后重启。
    // 用后台线程而不是让界面再调一次："界面卡住"不该把软件留在没有库的状态。
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1200));
        handle.restart();
    });
    Ok(outcome)
}

/// 让作者从磁盘上挑一个库文件（备份位置没配过 / 手上只有一份 `yanmo.db` 时用）。
///
/// 窗口标题与筛选项由界面传进来——**壳不产界面文案**。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_pick(title: String, filter_label: String) -> Option<String> {
    pick_database_file(&title, &filter_label)
}

#[cfg(windows)]
fn pick_database_file(title: &str, filter_label: &str) -> Option<String> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    // 过滤器是"成对的 `\0` 串，最后再来一个 `\0` 收尾"：标签\0*.db\0\0
    let mut filter = wide(filter_label);
    filter.extend(wide("*.db"));
    filter.push(0);
    let title = wide(title);
    let default_ext = wide("db");
    let mut file = vec![0u16; 4096];

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    // 挂到当前前台窗口上，对话框才会出现在研墨上面而不是背后
    ofn.hwndOwner = unsafe { GetForegroundWindow() };
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = default_ext.as_ptr();
    // OFN_NOCHANGEDIR：不加的话对话框会把**进程的当前目录**改到选中文件那一层，必须拦住
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None; // 作者取消（或系统不给对话框）：静默回去，什么都不做
    }
    let end = file.iter().position(|unit| *unit == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&file[..end]))
}

#[cfg(not(windows))]
fn pick_database_file(_title: &str, _filter_label: &str) -> Option<String> {
    None
}
