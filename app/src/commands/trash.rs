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

/// 恢复一段（整棵子树 + 还在回收站里的父链），返回恢复的节点数。
#[tauri::command(rename_all = "snake_case")]
pub fn restore_node(data: State<'_, AppData>, node_id: i64) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.restore_node(node_id))
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
