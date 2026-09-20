//! 灵感命令域：**一池碎片里，此刻最该看哪几条**，以及把它落到哪儿去。
//!
//! 与创作流那条线的分工：碎片的记 / 看 / 删 / 捞回归 [`crate::commands::fragment`]；
//! 这里只管"排出来"与"采纳 / 丢弃"——引力的公式与候选都来自核心
//! （[`yanmo_core::store::Spark`]），壳这一层不掺任何判断。
//!
//! 与叩问那条线的分工：那边是"机制挑问题问你"，这边是"机制替你把这堆碎片排个序"。
//! 两条线的落点集合**故意不同**（灵感多一档"事件池"，见 [`SparkTarget`]）。
//!
//! 命令粒度守则（与 [`crate::commands`] 一致）：看一眼、采纳、丢弃，
//! 外加碰撞那三下（看一眼、存下、跳过 / 撤销跳过）。
//! "采纳"是一个动作而不是四个命令：落点是它的参数（四种落点共用一条实现）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::SparkTarget;
use yanmo_core::store::{
    AdoptReceipt, Collision, CollisionReceipt, Spark, Store, COLLISIONS_PER_BOARD,
    SPARKS_PER_BOARD,
};

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

/// 看一眼**碰撞**：碎片与大纲元素（章纲 / 伏笔 / 人物卡）里，哪两头值得放一起撞一下。
///
/// 规则是确定性的（同输入同输出）：同处的（有共用锚点）优先，其次相隔的；
/// 一屏里一头只上一场；撞过的组合与作者跳过的不再出现；元素×元素不摆。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_collisions(
    data: State<'_, AppData>,
    work_id: i64,
    node_id: Option<i64>,
) -> Result<Vec<Collision>, ApiError> {
    crate::acceptance::note_command("inspire_collisions");
    data.with_store(|store| store.spark_collisions(work_id, node_id, COLLISIONS_PER_BOARD))
}

/// **存下撞出来的那一句**：写成一张双源溯源的灵感卡。
///
/// `left` / `right` 是**规范写法**（`spark:12` / `plan:3` / `foreshadow:5` / `entity:7`）——
/// 界面把核心给的那份原样交回来，不自己拼。
/// 关系（同处 / 相隔）由核心自己算——界面不必报，也就报不错。
/// 空句子 / 拿自己撞自己 / 两头都是元素 / 不属于同一本书，都当场拒。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_collide(
    data: State<'_, AppData>,
    left: String,
    right: String,
    body: String,
    node_id: Option<i64>,
) -> Result<CollisionReceipt, ApiError> {
    crate::acceptance::note_command("inspire_collide");
    data.with_store(|store| store.save_collision(&left, &right, &body, node_id, "author"))
}

/// 跳过这一对（**落盘**：下一次挑对还认它）。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_skip_collision(
    data: State<'_, AppData>,
    work_id: i64,
    left: String,
    right: String,
    node_id: Option<i64>,
) -> Result<Vec<Collision>, ApiError> {
    crate::acceptance::note_command("inspire_skip_collision");
    data.with_store(|store| store.skip_collision(work_id, &left, &right, node_id, "author"))
}

/// 撤销一次跳过（跳错了要能捡回来）。
#[tauri::command(rename_all = "snake_case")]
pub fn inspire_unskip_collision(
    data: State<'_, AppData>,
    work_id: i64,
    left: String,
    right: String,
    node_id: Option<i64>,
) -> Result<Vec<Collision>, ApiError> {
    crate::acceptance::note_command("inspire_unskip_collision");
    data.with_store(|store| store.unskip_collision(work_id, &left, &right, node_id, "author"))
}
