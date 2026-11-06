//! 外部文本导入：粘贴 / 选一个 txt·md → 碎片池。
//!
//! 业务（怎么切、怎么去重、怎么记账）全在核心 `store::fragment_import`；
//! 这里只做「读文件 / 转参数 / 回结果」。**一个字都不改正文。**

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::FragmentKind;
use yanmo_core::store::Store;

/// 一次导入最多读多大的文件（与核心的 200 万字上限对齐，留出编码/换行余量）。
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// 挑选出来的文本文件（界面只拿到名字与内容，**拿不到路径**）。
#[derive(Debug, Serialize)]
pub struct TextFileDto {
    pub name: String,
    pub text: String,
}

/// 分批预览：会切成几条、前几条长什么样。
#[derive(Debug, Serialize)]
pub struct ImportPreviewDto {
    pub count: usize,
    pub total_chars: usize,
    pub samples: Vec<String>,
}

/// 让作者挑一个 txt / md（路径只在壳里；界面拿到的是内容）。
#[tauri::command(rename_all = "snake_case")]
pub fn pick_text_file(title: String, filter_label: String) -> Option<TextFileDto> {
    let path = pick_file(&title, &filter_label)?;
    read_text(&path).ok()
}

fn read_text(path: &Path) -> Result<TextFileDto, ApiError> {
    let meta = std::fs::metadata(path).map_err(|error| {
        ApiError::with("shell.text_file_failed", [("path", path.display().to_string())])
            .caused_by(error.to_string())
    })?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(ApiError::with(
            "shell.text_file_too_large",
            [("path", path.display().to_string())],
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        ApiError::with("shell.text_file_failed", [("path", path.display().to_string())])
            .caused_by(error.to_string())
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        ApiError::with("shell.text_file_not_utf8", [("path", path.display().to_string())])
    })?;
    Ok(TextFileDto {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        text: text.trim_start_matches('\u{feff}').to_string(),
    })
}

/// 分批预览：**不落库**，只回答"会切成几条"。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_import_preview(text: String, mode: String) -> Result<ImportPreviewDto, ApiError> {
    let items = yanmo_core::store::split_import_text(&text, &mode).map_err(ApiError::from)?;
    let total_chars: usize = items.iter().map(|body| body.chars().count()).sum();
    let samples = items
        .iter()
        .take(5)
        .map(|body| {
            let mut sample: String = body.chars().take(80).collect();
            if body.chars().count() > 80 {
                sample.push('…');
            }
            sample
        })
        .collect();
    Ok(ImportPreviewDto { count: items.len(), total_chars, samples })
}

/// 真正导入：切好 → 核心去重 + 记账。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_import(
    data: State<'_, AppData>,
    work_id: i64,
    kind: String,
    label: String,
    text: String,
    mode: String,
) -> Result<yanmo_core::store::ImportOutcome, ApiError> {
    let items = yanmo_core::store::split_import_text(&text, &mode).map_err(ApiError::from)?;
    let kind = FragmentKind::parse(&kind).map_err(ApiError::from)?;
    data.with_store(|store: &mut Store| {
        Ok(store.import_fragments(work_id, kind, &label, "pasted", &items)?)
    })
}

/// 撤销一整批导入（只软删这一批碎片，不碰正文）。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_import_undo(
    data: State<'_, AppData>,
    batch_id: String,
) -> Result<i64, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.undo_import(&batch_id)?))
}

/// 这本书最近的导入批次。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_import_batches(
    data: State<'_, AppData>,
    work_id: i64,
) -> Result<Vec<yanmo_core::store::ImportBatch>, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.import_batches(work_id, 10)?))
}

/// 让作者挑一个文本文件（与"选字体 / 选背景"同一手法：**只在壳里调**）。
#[cfg(windows)]
fn pick_file(title: &str, filter_label: &str) -> Option<PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let mut filter = wide(filter_label);
    filter.extend(wide("*.txt;*.md"));
    filter.push(0);
    let title = wide(title);
    let mut file = vec![0u16; 4096];

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = unsafe { GetForegroundWindow() };
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if unsafe { GetOpenFileNameW(&mut ofn) } == 0 {
        return None;
    }
    let end = file.iter().position(|unit| *unit == 0).unwrap_or(0);
    Some(PathBuf::from(String::from_utf16_lossy(&file[..end])))
}

#[cfg(not(windows))]
fn pick_file(_title: &str, _filter_label: &str) -> Option<PathBuf> {
    None
}
