//! 编译命令域：一份原稿 → 一种成品（投稿版 docx / 分章 txt / 合并 txt）。
//!
//! 渲染全在核心（纯函数、可单测、可被命令行驱动）；这里只做参数转换与落盘。
//! 与排版清理同一条纪律：**先说清会生成哪些文件，再动手**（`compile_preview` / `compile_work`）。
//!
//! 产物落进**这一种预设自己的子目录**，并且只清那个子目录里的旧产物——换一种编译不会把
//! 上一种的产物（作者刚拿去投稿的那份）删掉。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::compile::{compile, CompileOptions, Preset};
use yanmo_core::store::Store;

/// 预设清单：界面按码取名字，也按这里给的默认值填参数（**默认值只有核心一处**）。
#[derive(Debug, Serialize)]
pub struct PresetDto {
    pub code: &'static str,
    /// 这一种要不要卡正文上限（界面据此决定显不显示"前 N 字"那个输入框）
    pub caps_body: bool,
    /// 默认上限（界面拿它当输入框初值）
    pub default_body_limit: i64,
    /// 默认附不附大纲
    pub default_outline: bool,
}

/// 预设清单（稳定代码 + 默认参数）。
#[tauri::command(rename_all = "snake_case")]
pub fn compile_presets() -> Vec<PresetDto> {
    Preset::ALL
        .iter()
        .map(|preset| {
            let defaults = CompileOptions::defaults_for(*preset);
            PresetDto {
                code: preset.as_str(),
                caps_body: preset.caps_body(),
                default_body_limit: defaults.body_limit.unwrap_or(0),
                default_outline: defaults.with_outline,
            }
        })
        .collect()
}

/// 一个将要生成的文件（界面只展示名字与大小，不碰文件系统）。
#[derive(Debug, Serialize)]
pub struct CompileFileDto {
    /// 相对导出目录的路径（含预设子目录）
    pub path: String,
    pub bytes: usize,
}

/// 一次编译的回执。
#[derive(Debug, Serialize)]
pub struct CompileAckDto {
    /// 落在哪个文件夹（界面只展示）
    pub path: String,
    pub files: Vec<CompileFileDto>,
    /// 顺手清掉了几个上次留下的旧产物（改了名、删了章之后的孤儿）
    pub removed: usize,
}

/// 参数落定：不卡上限的预设一律按"全书"（界面给了也不算数，免得界面上多一个没用的开关）。
///
/// `body_limit`：`None` = 不限（全书）。
fn options_of(preset: Preset, body_limit: Option<i64>, with_outline: Option<bool>) -> CompileOptions {
    let defaults = CompileOptions::defaults_for(preset);
    CompileOptions {
        body_limit: preset.caps_body().then_some(body_limit).flatten(),
        with_outline: with_outline.unwrap_or(defaults.with_outline),
    }
}

/// 会生成哪些文件（**不落盘**）：按"编译"之前先看清。
#[tauri::command(rename_all = "snake_case")]
pub fn compile_preview(
    data: State<'_, AppData>,
    work_id: i64,
    preset: String,
    body_limit: Option<i64>,
    with_outline: Option<bool>,
) -> Result<Vec<CompileFileDto>, ApiError> {
    let preset = Preset::parse(&preset).map_err(ApiError::from)?;
    let options = options_of(preset, body_limit, with_outline);
    data.with_store(|store: &mut Store| {
        Ok(compile(store, work_id, preset, &options)?
            .into_iter()
            .map(|file| CompileFileDto { path: file.relative_path, bytes: file.content.len() })
            .collect())
    })
}

/// 真编译：渲染 + 落盘。
#[tauri::command(rename_all = "snake_case")]
pub fn compile_work(
    data: State<'_, AppData>,
    work_id: i64,
    preset: String,
    body_limit: Option<i64>,
    with_outline: Option<bool>,
) -> Result<CompileAckDto, ApiError> {
    let preset = Preset::parse(&preset).map_err(ApiError::from)?;
    let options = options_of(preset, body_limit, with_outline);
    let (title, files) = data.with_store(|store: &mut Store| {
        let title = store.get_work(work_id)?.title;
        Ok((title, compile(store, work_id, preset, &options)?))
    })?;
    let extension = if preset == Preset::SubmissionDocx { "docx" } else { "txt" };
    let outcome = data.write_compile(&title, preset.folder(), extension, &files)?;
    Ok(CompileAckDto {
        path: outcome.dir.display().to_string(),
        files: files
            .iter()
            .map(|file| CompileFileDto { path: file.relative_path.clone(), bytes: file.content.len() })
            .collect(),
        removed: outcome.removed,
    })
}
