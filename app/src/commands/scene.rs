//! 「四格」命令域：**视角 / 目标 / 冲突 / 结果**的读写。
//!
//! 与编辑器分开的理由：四格是**低频手填字段**，不参与正文的落盘防抖与指纹校验
//! （与"章纲一句话"同一条路）；它跟正文不是同一个变化理由，别把编辑器那一份搅大。
//!
//! 规则在核心（`store::scene_card`）：**凡承载正文的节点都有这四格**
//! （章 / 节 / 单篇 / 场景卡——大纲表那一行就直接填它），卷没有，会被当场拒
//! （`node.no_fields`）。

use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::SceneFields;

/// 存一个节点的四格，返回**库里真有的那一份**。
///
/// 与"章纲一句话"同一条路：低频手填字段，不参与正文的落盘防抖与指纹校验，
/// 也不动正文一个字节。
#[tauri::command(rename_all = "snake_case")]
pub fn save_node_fields(
    data: State<'_, AppData>,
    node_id: i64,
    pov: String,
    goal: String,
    conflict: String,
    outcome: String,
) -> Result<SceneFields, ApiError> {
    crate::acceptance::note_command("save_node_fields");
    data.with_store(|store| {
        store.save_scene_fields(
            &SceneFields { node_id, pov, goal, conflict, outcome },
            "author",
        )
    })
}
