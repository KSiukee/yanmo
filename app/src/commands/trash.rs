//! 回收站命令：列出 / 恢复 / 彻底删除 / 清空。
//!
//! 与别的域一样，命令只做"参数转换 + 转交核心"：**恢复的语义（整棵子树 + 父链、
//! 彻底删除只对回收站里的东西开放）全在核心那一份实现里**，界面照着做就行。

use serde::Serialize;
use tauri::State;

use crate::storage::AppData;
use yanmo_core::store::{Store, TrashEntry};

/// 回收站里的一项。
#[derive(Debug, Serialize)]
pub struct TrashEntryDto {
    /// 「work」= 整本书；「node」= 书里被删的一段
    pub kind: String,
    pub id: i64,
    pub title: String,
    pub work_id: i64,
    pub work_title: String,
    pub deleted_at: i64,
    /// 跟着一起进来 / 会一起回去的节点数（含它自己）
    pub nodes: i64,
}

fn to_dto(entry: TrashEntry) -> TrashEntryDto {
    TrashEntryDto {
        kind: entry.kind.as_str().to_string(),
        id: entry.id,
        title: entry.title,
        work_id: entry.work_id,
        work_title: entry.work_title,
        deleted_at: entry.deleted_at,
        nodes: entry.nodes,
    }
}

/// 回收站里有什么（整本书在前，书里被删的段在后）。
#[tauri::command]
pub fn list_trash(data: State<'_, AppData>) -> Result<Vec<TrashEntryDto>, String> {
    data.with_store(|store: &mut Store| Ok(store.list_trash()?.into_iter().map(to_dto).collect()))
}

/// 恢复一本书。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_work(data: State<'_, AppData>, work_id: i64) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.restore_work(work_id))
}

/// 同级里与它重名的那一个（恢复前摆给作者看）。
#[derive(Debug, Serialize)]
pub struct NameClashDto {
    pub id: i64,
    pub title: String,
    pub word_count: i64,
}

/// 恢复**之前**的交代：回到哪、会不会与谁重名。
#[derive(Debug, Serialize)]
pub struct RestorePreviewDto {
    pub work_id: i64,
    pub work_title: String,
    /// 原来的父级；null = 根级
    pub parent_title: Option<String>,
    /// 原来在第几位（从 1 起）
    pub index: i64,
    pub name_clashes: Vec<NameClashDto>,
}

/// 恢复前先看一眼：有没有同名冲突（有冲突时界面要让作者拿主意）。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_preview(data: State<'_, AppData>, node_id: i64) -> Result<RestorePreviewDto, String> {
    data.with_store(|store: &mut Store| {
        let preview = store.restore_preview(node_id)?;
        Ok(RestorePreviewDto {
            work_id: preview.work_id,
            work_title: preview.work_title,
            parent_title: preview.parent_title,
            index: preview.index,
            name_clashes: preview
                .name_clashes
                .into_iter()
                .map(|clash| NameClashDto {
                    id: clash.id,
                    title: clash.title,
                    word_count: clash.word_count,
                })
                .collect(),
        })
    })
}

/// 恢复一段（整棵子树 + 还在回收站里的父链），返回恢复的节点数。
///
/// `title` 是作者在"有重名"时给的新名字——**名字永远由他给**；留空 = 照原样恢复。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_node(
    data: State<'_, AppData>,
    node_id: i64,
    title: Option<String>,
) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.restore_node(node_id, title.as_deref()))
}

/// 彻底删除一段（**不可恢复**），返回删掉的节点数。
#[tauri::command(rename_all = "snake_case")]
pub fn purge_node(data: State<'_, AppData>, node_id: i64) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.purge_node(node_id))
}

/// 彻底删除一本书（**不可恢复**），返回删掉的节点数。
#[tauri::command(rename_all = "snake_case")]
pub fn purge_work(data: State<'_, AppData>, work_id: i64) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.purge_work(work_id))
}

/// 清空回收站，返回清掉的项数。
#[tauri::command]
pub fn empty_trash(data: State<'_, AppData>) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.empty_trash())
}
