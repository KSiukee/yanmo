//! 伏笔命令域：**埋下的一条线头**——记、改、走一步状态、删。
//!
//! 与设定卡的分工：设定卡是**人物 / 设定**（有名字与键值），伏笔是**线头**（埋在哪、收没收）。
//! 两者都是大纲体检的数据源，住在同一个「设定」面板的两个页签里。
//!
//! 三个口径（与核心一致）：
//!
//! - **"不写了"是正经结局**：不是失败，也不该再被念；
//! - **走一步要合法**：非法边当场拒（界面上的按钮由核心的 `next_states` 摆）；
//! - **锚点得对得上**：埋点 / 收点必须是这本书里还在的节点（错的说人话，不露数据库的话）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::{Foreshadow, ForeshadowState, NewForeshadow};
use yanmo_core::store::Store;

/// 一章的**名分**（伏笔要挂到它上面：表单里选、列表里显示）。
///
/// 同一次给全：伏笔那一屏既要列"埋在第几章"，也要让作者选一章——
/// 分两次问只会让同一屏前后对不上（与别的面板同一条口径）。
#[derive(Debug, Serialize)]
pub struct ChapterRefDto {
    pub id: i64,
    /// **渲染后**的名字（`第3章` 这种）——界面直接显示，不再自己拼号
    pub title: String,
    /// 第几章（1 起；阅读顺序）
    pub index: i64,
}

/// 一整屏：这本书的伏笔 + 按状态分的数字 + 章的名分。
#[derive(Debug, Serialize)]
pub struct ForeshadowBoardDto {
    pub items: Vec<Foreshadow>,
    pub planted: usize,
    pub collected: usize,
    pub dropped: usize,
    pub chapters: Vec<ChapterRefDto>,
}

fn board(store: &Store, work_id: i64) -> yanmo_core::Result<ForeshadowBoardDto> {
    let items = store.foreshadows(work_id, None)?;
    let count = |state: ForeshadowState| items.iter().filter(|item| item.state == state).count();
    let chapters = store
        .text_spine(work_id)?
        .into_iter()
        .enumerate()
        .map(|(at, chapter)| ChapterRefDto {
            id: chapter.id,
            title: chapter.title,
            index: at as i64 + 1,
        })
        .collect();
    Ok(ForeshadowBoardDto {
        planted: count(ForeshadowState::Planted),
        collected: count(ForeshadowState::Collected),
        dropped: count(ForeshadowState::Dropped),
        items,
        chapters,
    })
}

/// 列这本书的伏笔（埋着的排前面）。
#[tauri::command(rename_all = "snake_case")]
pub fn foreshadow_list(data: State<'_, AppData>, work_id: i64) -> Result<ForeshadowBoardDto, ApiError> {
    crate::acceptance::note_command("foreshadow_list");
    data.with_store(|store| board(store, work_id))
}

/// 记一条（状态从「埋着」开始；正文空着会被拒）。
#[tauri::command(rename_all = "snake_case")]
pub fn foreshadow_create(
    data: State<'_, AppData>,
    work_id: i64,
    body: String,
    planted_node: Option<i64>,
    note: Option<String>,
) -> Result<ForeshadowBoardDto, ApiError> {
    crate::acceptance::note_command("foreshadow_create");
    data.with_store(|store| {
        store.create_foreshadow(
            &NewForeshadow {
                work_id,
                body,
                planted_node,
                note: note.unwrap_or_default(),
            },
            "author",
        )?;
        board(store, work_id)
    })
}

/// 改一条（正文 / 埋点 / 备注；**状态不动**——状态走 `foreshadow_move`）。
#[tauri::command(rename_all = "snake_case")]
pub fn foreshadow_update(
    data: State<'_, AppData>,
    id: i64,
    body: String,
    planted_node: Option<i64>,
    note: Option<String>,
) -> Result<ForeshadowBoardDto, ApiError> {
    crate::acceptance::note_command("foreshadow_update");
    data.with_store(|store| {
        let item = store.update_foreshadow(id, &body, planted_node, &note.unwrap_or_default(), "author")?;
        board(store, item.work_id)
    })
}

/// 走一步状态：收到哪一章（`collected_node` 可给可不给）。
#[tauri::command(rename_all = "snake_case")]
pub fn foreshadow_move(
    data: State<'_, AppData>,
    id: i64,
    to: String,
    collected_node: Option<i64>,
) -> Result<ForeshadowBoardDto, ApiError> {
    crate::acceptance::note_command("foreshadow_move");
    data.with_store(|store| {
        let item = store.move_foreshadow(id, ForeshadowState::parse(&to)?, collected_node, "author")?;
        board(store, item.work_id)
    })
}

/// 删一条（**软删**：库里还留着）。
#[tauri::command(rename_all = "snake_case")]
pub fn foreshadow_delete(data: State<'_, AppData>, id: i64) -> Result<ForeshadowBoardDto, ApiError> {
    crate::acceptance::note_command("foreshadow_delete");
    data.with_store(|store| {
        let gone = store.delete_foreshadow(id, "author")?;
        board(store, gone.work_id)
    })
}
