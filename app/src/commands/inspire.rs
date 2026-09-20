//! 灵感命令域：**一池碎片里，此刻最该看哪几条**，以及把它落到哪儿去。
//!
//! 与创作流那条线的分工：碎片的记 / 看 / 删 / 捞回归 [`crate::commands::fragment`]；
//! 这里只管"排出来"与"采纳 / 丢弃"——引力的公式与候选都来自核心
//! （[`yanmo_core::store::Spark`]），壳这一层不掺任何判断。
//!
//! 与叩问那条线的分工：那边是"机制挑问题问你"，这边是"机制替你把这堆碎片排个序"。
//! 两条线的落点集合**故意不同**（灵感多一档"事件池"，见 [`SparkTarget`]）。
//!
//! 命令粒度守则（与 [`crate::commands`] 一致）：三个就够——看一眼、采纳、丢弃。
//! "采纳"是一个动作而不是四个命令：落点是它的参数（四种落点共用一条实现）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::SparkTarget;
use yanmo_core::store::{AdoptReceipt, Spark, Store, SPARKS_PER_BOARD};

/// 一屏灵感：**排好序的那几条**。
///
/// 一次给全（与创作流面板同一条口径）：这块面板上只有这一件事，分两次问只会让
/// "采纳完这一条之后下一条是谁"对不上。
#[derive(Debug, Serialize)]
pub struct InspireBoardDto {
    /// 按引力排好的候选（新的、重要的、跟当前这一章有关的在前）
    pub sparks: Vec<Spark>,
    /// 落点闭集（界面按它摆按钮，**不自己写一份**）
    pub targets: Vec<SparkTarget>,
}

fn board(store: &Store, work_id: i64, node_id: Option<i64>) -> yanmo_core::Result<InspireBoardDto> {
    Ok(InspireBoardDto {
        sparks: store.spark_candidates(work_id, node_id, SPARKS_PER_BOARD)?,
        targets: SparkTarget::ALL.to_vec(),
    })
}

/// 看一眼：这本书里此刻最该看的几条灵感。
///
/// `node_id`＝作者现在打开着哪一章（可不给）。它**只影响排序**：跟这一章有关联的排前面。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_board(
    data: State<'_, AppData>,
    work_id: i64,
    node_id: Option<i64>,
) -> Result<InspireBoardDto, ApiError> {
    crate::acceptance::note_command("inspire_board");
    data.with_store(|store| board(store, work_id, node_id))
}

/// 采纳一条：记下这本账，并按落点把它送出去。
///
/// - `target`：`body`（正文段落）/ `outline`（章纲）/ `scene`（场景卡）/ `event`（事件池）；
///   空串当正文。认不出的取值当场拒。
/// - `node_id`：`outline` 与 `scene` 要（落到哪一章），其余忽略。
/// - `title`：场景卡的名字（空着就交给命名规则）。
///
/// **落正文时字由界面插进编辑会话**（与叩问落正文同一条路）：回执里的 `text` 就是那一段，
/// 界面先插字、再拿这本账——顺序是刻意的，插字是作者看得见的动作，记账是账。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_adopt(
    data: State<'_, AppData>,
    id: i64,
    target: String,
    node_id: Option<i64>,
    title: Option<String>,
) -> Result<AdoptReceipt, ApiError> {
    crate::acceptance::note_command("inspire_adopt");
    data.with_store(|store| {
        store.adopt_spark(id, &target, node_id, &title.unwrap_or_default(), "author")
    })
}

/// 丢弃一条（**软删**，能捞回；不动"被采纳过几次"那本账）。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_discard(
    data: State<'_, AppData>,
    id: i64,
    node_id: Option<i64>,
) -> Result<Vec<Spark>, ApiError> {
    crate::acceptance::note_command("inspire_discard");
    data.with_store(|store| store.discard_spark(id, node_id, "author"))
}
