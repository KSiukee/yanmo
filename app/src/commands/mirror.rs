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

/// 处置一条**待定夺**的事：把磁盘上那一份**收进研墨**（`adopt`），或用研墨里的版本**盖回去**
/// （`overwrite`）。
///
/// **不收路径**：只收"第几条"（清单是壳自己产出来的）+ 那个节点 id 做核对——界面从头到尾
/// 拿不到、也递不进一个可操作的文件路径（与"命令不接受路径参数"同一条纪律）。
///
/// 两条路最后都落在同一处：**把磁盘上那一份删掉，让镜像下一轮按库里的字重写**。
/// 这样"写盘"永远只有镜像那一处，格式与原子写都只有一份口径。
#[tauri::command(rename_all = "snake_case")]
pub fn mirror_resolve(
    data: State<'_, AppData>,
    index: usize,
    node_id: i64,
    action: String,
) -> Result<MirrorStatus, ApiError> {
    let handle = data.mirror().ok_or_else(|| ApiError::new("shell.mirror_unavailable"))?;
    let adopt = match action.as_str() {
        "adopt" => true,
        "overwrite" => false,
        other => {
            return Err(ApiError::with(
                "shell.mirror_unknown_action",
                [("value", other.to_string())],
            ));
        }
    };

    let issues = handle.status().issues;
    let issue = issues
        .get(index)
        .ok_or_else(|| ApiError::new("shell.mirror_issue_gone"))?;
    // 清单可能刚被别的处置刷新过：序号与节点对不上就明确报"这一条已经不在了"，
    // **绝不按一个过期的序号去动文件**。
    if issue.kind == "untracked" || issue.node_id != node_id {
        return Err(ApiError::new("shell.mirror_issue_gone"));
    }

    let root = data.mirror_root();
    let path = root.join(&issue.relative_path);
    // 安全阀：清单是我们自己产的，仍然钉一道——绝不出镜像根
    if !path.starts_with(&root) {
        return Err(ApiError::new("shell.mirror_issue_gone"));
    }

    if adopt {
        let text = std::fs::read_to_string(&path).map_err(|error| {
            ApiError::with("shell.mirror_read_failed", [("path", path.display().to_string())])
                .caused_by(error)
        })?;
        let body = yanmo_core::store::mirror_body_of(&text);
        data.with_store(|store| store.adopt_mirror_body(issue.node_id, &body))?;
    }

    match std::fs::remove_file(&path) {
        Ok(()) => {}
        // 已经不在了：目标状态就是"它不在"，下一轮照样按库里的字写出来
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(ApiError::with(
                "shell.mirror_remove_failed",
                [("path", path.display().to_string())],
            )
            .caused_by(error));
        }
    }

    // 立刻对一遍，不等下一次巡检（作者点了按钮就该看到它变了）
    data.force_mirror();
    status_of(&data)
}
