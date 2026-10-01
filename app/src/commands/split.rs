//! 拆章命令域：在作者指定的切点把一章切成两章，以及撤销那一次。
//!
//! 单独一个文件的原因与「章长」那一族同一条：**它要改正文**，是这一族里唯一会动的动作，
//! 变化理由跟"边写边存"（`commands::editor`）不是一回事。切法与留底全在核心
//! （`store::split_chapter`），这里只做参数转换、并把新章那一屏交回界面。
//!
//! 两条口径：
//! - **只由作者触发**：没有任何自动路径会调到这里（提醒只报事实，拆不拆由作者定）；
//! - **先落盘再拆由界面负责**：核心切的是库里那一份，界面上没存完的字不在里面——
//!   界面在调这一条之前先 flush（见 `editor/chapter-split.ts`）。

use serde::Serialize;
use tauri::State;

use super::editor::{snapshot_of, sweep_question_requeues, EditorSnapshot};
use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::SplitReceipt;

/// 拆章的回执：新章那一屏（界面直接切过去）+ 撤销要的那几样（原样收着交回来）。
#[derive(Debug, Serialize)]
pub struct SplitAck {
    pub new_chapter: EditorSnapshot,
    pub receipt: SplitReceipt,
}

/// 在**第 `at` 段之前**把这一章切成两章（**只由作者触发，绝不自动拆**）。
///
/// 切点由界面按光标算好（口径：光标所在那一段的开头），核心只认段落下标并校验它合法。
/// 返回**新章那一屏**，界面照"新建一章"那条路切过去即可
/// （光标落在新章开头 = 作者刚才放切点的地方）。
#[tauri::command(rename_all = "snake_case")]
pub fn chapter_split(
    data: State<'_, AppData>,
    node_id: i64,
    at: usize,
    title: String,
) -> Result<SplitAck, ApiError> {
    crate::acceptance::note_command("chapter_split");
    data.with_store(|store| {
        let receipt = store.split_chapter(node_id, at, &title)?;
        let target = store.editor_target(receipt.new_node_id)?;
        store.note_open_node(target.node_id)?;
        sweep_question_requeues(store, target.work_id);
        Ok(SplitAck { new_chapter: snapshot_of(store, target)?, receipt })
    })
}

/// 撤销一次拆章：两章合回原章，新章进回收站；回**原章**那一屏（界面切回去）。
#[tauri::command(rename_all = "snake_case")]
pub fn chapter_split_undo(
    data: State<'_, AppData>,
    old_node_id: i64,
    new_node_id: i64,
    snapshot_id: i64,
) -> Result<EditorSnapshot, ApiError> {
    crate::acceptance::note_command("chapter_split_undo");
    data.with_store(|store| {
        store.undo_split(old_node_id, new_node_id, snapshot_id)?;
        let target = store.editor_target(old_node_id)?;
        store.note_open_node(target.node_id)?;
        sweep_question_requeues(store, target.work_id);
        snapshot_of(store, target)
    })
}
