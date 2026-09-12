//! 书架命令：**多作品是默认形态**——列书、建书、改名、删书。
//!
//! 分工与别的域一致：命令只做"参数转换 + 转交核心"，**不知道界面长什么样**。
//! "打开某一本书"归编辑域（它要回一份编辑器快照），这里只碰作品本身。
//!
//! 数据上一条纪律：**"当前作品"不是全局单例**——每一本书各自记着自己的"读到哪了"，
//! 切走再切回来要回得到原位（记录在核心，见 `yanmo_core::store`）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::{WorkKind, WorkLanguage};
use yanmo_core::store::{ExportFormat, ShelfEntry, Store};

/// 书架的一行。
#[derive(Debug, Serialize)]
pub struct ShelfEntryDto {
    pub id: i64,
    /// 「长篇 / 短篇集 / 单篇」
    pub kind: String,
    pub title: String,
    /// 这本书里章的个数（**单篇文章是 0**，界面此时只报字数）
    pub chapters: i64,
    /// 字数合计（各章预聚合字数之和，不扫正文）
    pub word_count: i64,
    /// 最近打开时间（unix 毫秒）；从没打开过是 null
    pub opened_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn to_dto(entry: ShelfEntry) -> ShelfEntryDto {
    ShelfEntryDto {
        id: entry.work.id,
        kind: entry.work.kind.as_str().to_string(),
        title: entry.work.title,
        chapters: entry.chapters,
        word_count: entry.word_count,
        opened_at: entry.work.opened_at,
        created_at: entry.work.created_at,
        updated_at: entry.work.updated_at,
    }
}

/// 书架：最近打开的在前，带上每本书的章数与字数。
#[tauri::command]
pub fn list_shelf(data: State<'_, AppData>) -> Result<Vec<ShelfEntryDto>, ApiError> {
    data.with_store(|store: &mut Store| {
        Ok(store.shelf()?.into_iter().map(to_dto).collect())
    })
}

/// 新建一本书，返回它的 id（界面接着调"打开这本书"就能直接进去写）。
#[tauri::command(rename_all = "snake_case")]
pub fn create_work(
    data: State<'_, AppData>,
    kind: String,
    title: String,
) -> Result<i64, ApiError> {
    data.with_store(|store: &mut Store| {
        Ok(store.create_work(WorkKind::parse(&kind)?, &title)?.id)
    })
}

/// 给书改名。
#[tauri::command(rename_all = "snake_case")]
pub fn rename_work(
    data: State<'_, AppData>,
    work_id: i64,
    title: String,
) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| store.rename_work(work_id, &title))
}

/// 改作品语言的回执：改完的**语言**与**落定后的字数口径**。
///
/// 口径一起回，是因为界面上那个数字要立刻跟着变——而"改完该用哪个口径"的规则在核心
/// （作者选过就听作者的，没选过跟新语言的默认），界面不该自己算。
#[derive(Debug, Serialize)]
pub struct WorkLanguageAck {
    pub work_id: i64,
    pub language: String,
    pub word_caliber: &'static str,
}

/// 改作品语言（`zh` / `en` / `ja`）——**字数默认口径跟它走**。
///
/// 写完**回读一次**再把两个码回传：界面据此确认"真的存下去了"，也免得两边各说各话。
#[tauri::command(rename_all = "snake_case")]
pub fn set_work_language(
    data: State<'_, AppData>,
    work_id: i64,
    language: String,
) -> Result<WorkLanguageAck, ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_work_language(work_id, WorkLanguage::parse(&language)?)?;
        Ok(WorkLanguageAck {
            work_id,
            language: store.get_work(work_id)?.language.as_str().to_string(),
            word_caliber: store.word_caliber(work_id)?.as_str(),
        })
    })
}

/// 删掉一本书（**软删除**：正文与历史都留着，回收站接上后能捞回来）。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_work(data: State<'_, AppData>, work_id: i64) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| store.soft_delete_work(work_id))
}

/// 一次导出的回执。
#[derive(Debug, Serialize)]
pub struct ExportAckDto {
    /// 导到哪个文件夹（界面只展示，不碰文件系统）
    pub path: String,
    pub files: usize,
    /// 顺手清掉了几个上次导出留下的旧文件（改名 / 删章之后的孤儿）
    pub removed: usize,
}

/// 把一本书导出成 `txt`（分章，结构用目录表达）或 `json`（单文件，结构与正文都在里面）。
///
/// 落在"文档 / 研墨导出 / 书名"下；同样的内容不会重复写，旧文件会被清掉——
/// 所以这个文件夹可以放心交给 git 或同步盘。
#[tauri::command(rename_all = "snake_case")]
pub fn export_work(
    data: State<'_, AppData>,
    work_id: i64,
    format: String,
) -> Result<ExportAckDto, ApiError> {
    // `both` = 一次导出两种：分章 txt 给人接手，单个 json 给机器读回来
    let formats = if format == "both" {
        vec![ExportFormat::Text, ExportFormat::Json]
    } else {
        vec![ExportFormat::parse(&format).map_err(ApiError::from)?]
    };
    let (title, files) = data.with_store(|store: &mut Store| {
        let mut files = Vec::new();
        for one in &formats {
            files.extend(store.render_work(work_id, *one)?);
        }
        Ok((store.get_work(work_id)?.title, files))
    })?;
    let outcome = data.write_export(&title, &files)?;
    Ok(ExportAckDto {
        path: outcome.dir.display().to_string(),
        files: outcome.files,
        removed: outcome.removed,
    })
}
