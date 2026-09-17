//! 磁盘 `.md` 镜像的命令域：状态报告 / 开关 / 立即同步 / 打开文件夹。
//!
//! 命令**不接受路径参数**：镜像就是数据目录里的 `mirror/`，界面无从指定别处
//! （与导出、换位置同一条纪律：路径策略在壳）。
//!
//! 状态是"上次对完账的样子"，**读的是壳里那份内存报告**——所以这个命令很便宜，
//! 界面想多久问一次都行，不会因为问状态去碰数据库。

use tauri::State;

use crate::error::ApiError;
use crate::mirror::MirrorStatus;
use crate::storage::AppData;

/// 镜像现在什么样（界面拿它显示状态、冲突数与上次对上的时间）。
#[tauri::command(rename_all = "snake_case")]
pub fn mirror_status(data: State<'_, AppData>) -> Result<MirrorStatus, ApiError> {
    status_of(&data)
}

/// 开 / 关镜像。**关掉不删任何文件**——磁盘上那些 `.md` 是作者的东西；
/// 打开时立刻补一次全量对账（按下开关就该看到文件出现，而不是等下一次落盘）。
#[tauri::command(rename_all = "snake_case")]
pub fn mirror_set_enabled(
    data: State<'_, AppData>,
    enabled: bool,
) -> Result<MirrorStatus, ApiError> {
    data.with_store(|store| store.set_mirror_enabled(enabled))?;
    if enabled {
        data.force_mirror();
    } else if let Some(handle) = data.mirror() {
        // 关掉也投一次信号：状态那一栏立刻变成"关着"，不用等下一次巡检
        handle.poke();
    }
    status_of(&data)
}

/// 请它立刻做一次全量核对（不信账、逐份比过磁盘）。
///
/// 异步的：命令回来只代表"信号送到了"。界面隔一会儿再问一次状态就是。
#[tauri::command(rename_all = "snake_case")]
pub fn mirror_sync_now(data: State<'_, AppData>) -> Result<(), ApiError> {
    if data.mirror().is_none() {
        // 没有工作线程（验收模式）时说清楚，别让界面以为"点过了、正在跑"
        return Err(ApiError::new("shell.mirror_unavailable"));
    }
    data.force_mirror();
    Ok(())
}

/// 在文件管理器里打开镜像目录。
///
/// 目录还不存在就先建出来：作者看到"点了没反应"最难查，而一个空目录也是明确的回答。
#[tauri::command(rename_all = "snake_case")]
pub fn mirror_open_folder(data: State<'_, AppData>) -> Result<(), ApiError> {
    let root = data.mirror_root();
    std::fs::create_dir_all(&root).map_err(|error| {
        ApiError::with("shell.mirror_dir_create_failed", [("path", root.display().to_string())])
            .caused_by(error)
    })?;
    crate::open_folder::open(&root).map_err(|detail| {
        ApiError::with("shell.open_dir_failed", [("path", root.display().to_string())])
            .caused_by(detail)
    })
}

/// 取状态：有工作线程就用它的报告；没有（验收模式）就如实读一次开关，不编。
fn status_of(data: &AppData) -> Result<MirrorStatus, ApiError> {
    let root = data.mirror_root().display().to_string();
    match data.mirror() {
        Some(handle) => Ok(handle.status()),
        None => Ok(MirrorStatus {
            enabled: data.with_store(|store| store.mirror_enabled())?,
            root,
            ..MirrorStatus::default()
        }),
    }
}
