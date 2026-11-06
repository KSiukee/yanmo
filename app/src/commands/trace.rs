// i18n-allow-file: 目录名是文件系统里的名字（研墨留痕-…），由壳拼；界面文案在 locales 里
//! 创作留痕包：**核心打包 → 壳落盘**（作者选一个文件夹，一键出一份）。
//!
//! 与备份分开的理由：备份是"整库可恢复"，留痕包是"过程材料可自验"——
//! 一个给未来的自己，一个给编辑 / 平台 / 任何人看。包里**不含正文**（见核心那个模块的头）。
//!
//! 落盘边界：核心只产字节，路径与写文件都在壳里；界面拿到的只有"写到哪儿了"。
//! 目录名用书名 + 本地时间戳（`研墨留痕-长夜-20261105-1320`），重名也不会覆盖。

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;

/// 导出回执：写到哪儿、几个文件、几条事件、链头是什么。
#[derive(Debug, Serialize)]
pub struct TraceExport {
    pub dir: String,
    pub files: usize,
    pub events: usize,
    pub head: String,
}

fn write_failed(path: &Path, error: std::io::Error) -> ApiError {
    ApiError::from(yanmo_core::Error::Io(std::io::Error::new(
        error.kind(),
        format!("cannot write the trace package ({}): {error}", path.display()),
    )))
}

/// 打一份这本书的留痕包，写到作者选的文件夹里。
///
/// 作者取消返回 `None`（静默回去）；标题由界面传（壳不产界面文案）。
#[tauri::command(rename_all = "snake_case")]
pub fn trace_export(
    title: String,
    work_id: i64,
    tz_offset_minutes: i32,
    scope: String,
    data: State<'_, AppData>,
) -> Result<Option<TraceExport>, ApiError> {
    let Some(parent) = crate::pick_dir::pick(&title) else {
        return Ok(None);
    };
    let package = data.with_store(|store| {
        store.build_trace_package_with(work_id, yanmo_core::store::TraceScope::parse(&scope))
    })?;
    let stamp =
        yanmo_core::time::local_stamp(yanmo_core::time::now_millis(), tz_offset_minutes);
    let folder = parent.join(format!(
        "{}-{stamp}",
        yanmo_core::store::trace_folder_name(&package.title)
    ));
    std::fs::create_dir_all(&folder).map_err(|error| write_failed(&folder, error))?;
    for file in &package.files {
        let path = folder.join(&file.name);
        std::fs::write(&path, &file.bytes).map_err(|error| write_failed(&path, error))?;
    }
    Ok(Some(TraceExport {
        dir: folder.to_string_lossy().to_string(),
        files: package.files.len(),
        events: package.events,
        head: package.head,
    }))
}
