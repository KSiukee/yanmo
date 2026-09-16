//! 场景卡命令域：**四格**（视角 / 目标 / 冲突 / 结果）的读写。
//!
//! 与编辑器分开的理由：四格是**低频手填字段**，不参与正文的落盘防抖与指纹校验
//! （与"章纲一句话"同一条路）；它跟正文不是同一个变化理由，别把编辑器那一份搅大。
//!
//! 规则在核心（`store::scene_card`）：不是场景卡的节点会被当场拒（`node.not_scene`）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::SceneFields;

/// 一张场景卡给界面看的形状：名字 + 四格（`node_id` 是它的身份）。
#[derive(Debug, Serialize)]
pub struct SceneCardDto {
    pub node_id: i64,
    /// 树上那个名字（可能空着——界面说"还没起名"）
    pub title: String,
    pub fields: SceneFields,
}

/// 这本书里**所有场景卡**（按树里的顺序），带上各自的四格。
///
/// 给「大纲」面板那一页用：**在那儿就能把四格填了**，不必先切到那一章去。
#[tauri::command(rename_all = "snake_case")]
pub fn scene_list(data: State<'_, AppData>, work_id: i64) -> Result<Vec<SceneCardDto>, ApiError> {
    crate::acceptance::note_command("scene_list");
    data.with_store(|store| {
        Ok(store
            .scene_cards_of_work(work_id)?
            .into_iter()
            .map(|(node_id, title, fields)| SceneCardDto { node_id, title, fields })
            .collect())
    })
}

/// 存场景卡的四格（视角 / 目标 / 冲突 / 结果），返回**库里真有的那一份**。
///
/// 与"章纲一句话"同一条路：它是**低频的手填字段**，不参与正文的落盘防抖与指纹校验，
/// 也不动正文一个字节。不是场景卡的节点会被核心当场拒（`node.not_scene`）。
#[tauri::command(rename_all = "snake_case")]
pub fn save_scene_fields(
    data: State<'_, AppData>,
    node_id: i64,
    pov: String,
    goal: String,
    conflict: String,
    outcome: String,
) -> Result<SceneFields, ApiError> {
    crate::acceptance::note_command("save_scene_fields");
    data.with_store(|store| {
        store.save_scene_fields(
            &SceneFields { node_id, pov, goal, conflict, outcome },
            "author",
        )
    })
}
