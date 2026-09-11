//! 书架命令：**多作品是默认形态**——列书、建书、改名、删书。
//!
//! 分工与别的域一致：命令只做"参数转换 + 转交核心"，**不知道界面长什么样**。
//! "打开某一本书"归编辑域（它要回一份编辑器快照），这里只碰作品本身。
//!
//! 数据上一条纪律：**"当前作品"不是全局单例**——每一本书各自记着自己的"读到哪了"，
//! 切走再切回来要回得到原位（记录在核心，见 `yanmo_core::store`）。

use serde::Serialize;
use tauri::State;

use crate::storage::AppData;
use yanmo_core::model::WorkKind;
use yanmo_core::store::{ShelfEntry, Store};

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
pub fn list_shelf(data: State<'_, AppData>) -> Result<Vec<ShelfEntryDto>, String> {
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
) -> Result<i64, String> {
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
) -> Result<(), String> {
    data.with_store(|store: &mut Store| store.rename_work(work_id, &title))
}

/// 删掉一本书（**软删除**：正文与历史都留着，回收站接上后能捞回来）。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_work(data: State<'_, AppData>, work_id: i64) -> Result<(), String> {
    data.with_store(|store: &mut Store| store.soft_delete_work(work_id))
}
