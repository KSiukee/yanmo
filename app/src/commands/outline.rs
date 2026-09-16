//! 大纲命令域：**表的读数**、**整片粘贴**、**出场人物**，以及**体检**（只报告，不改稿）。
//!
//! 体检的规则全在核心的 `outline`（纯逻辑、零文案）；这里只做两件事：
//! 把发现包成界面认的形状（**带上指纹**，忽略标记认它），以及记下"这一处我知道了"。
//!
//! 三个口径：
//!
//! - **只读**：扫一遍不动一个字节（真正常见的动作是"点开看一眼"）；
//! - **零文案**：给的是规则码 + 参数 + 定位锚点，句子在界面字典（`outline.issue.*`）；
//! - **忽略是作者对自己清单的处置**：记下来（按书分），随时能撤销或全部重看。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::outline::OutlineIssue;
use yanmo_core::store::{CastMember, OutlineCell, OutlineRow, Store};

/// 一条发现给界面的形状（**带指纹**：忽略标记与"撤销忽略"都认它）。
#[derive(Debug, Serialize)]
pub struct OutlineIssueDto {
    /// 规则码（界面字典键：`outline.issue.<码>`）
    pub rule: String,
    /// 定位锚点（`entity:3` / `scene:9`）——点得动
    pub anchors: Vec<String>,
    /// 渲染句子用的取值（名字 / 属性键 / 缺了哪几格……）
    pub params: BTreeMap<String, String>,
    /// 这条问题的身份（忽略标记认它；同一条下一轮还是它）
    pub fingerprint: String,
}

impl From<&OutlineIssue> for OutlineIssueDto {
    fn from(issue: &OutlineIssue) -> Self {
        Self {
            rule: issue.rule.as_str().to_string(),
            anchors: issue.anchors.clone(),
            params: issue.params.clone(),
            fingerprint: issue.fingerprint(),
        }
    }
}

/// 体检的一整屏：全部发现（**含已忽略的**）+ 已经忽略的那些指纹。
///
/// 已忽略的也照发：面板要能列出"我忽略过的"并让作者捡回来——
/// 只发没忽略的那部分，界面上就再也找不到回头路。
#[derive(Debug, Serialize)]
pub struct OutlineBoardDto {
    pub issues: Vec<OutlineIssueDto>,
    pub dismissed: Vec<String>,
}

fn board(store: &Store, work_id: i64) -> yanmo_core::Result<OutlineBoardDto> {
    Ok(OutlineBoardDto {
        issues: store.outline_issues(work_id)?.iter().map(OutlineIssueDto::from).collect(),
        dismissed: store.dismissed_issues(work_id)?,
    })
}

/// 大纲表要读的那一屏：**整棵树铺平**，一行带上该有的列（四格 / 伏笔账 / 字数）。
///
/// 一次给全：界面不必为一屏发四次请求，也不会出现"同一屏里前后对不上"。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_rows(data: State<'_, AppData>, work_id: i64) -> Result<Vec<OutlineRow>, ApiError> {
    crate::acceptance::note_command("outline_rows");
    data.with_store(|store| store.outline_rows(work_id))
}

/// 扫一遍这本书的大纲（**只读**）。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_scan(data: State<'_, AppData>, work_id: i64) -> Result<OutlineBoardDto, ApiError> {
    crate::acceptance::note_command("outline_scan");
    data.with_store(|store| board(store, work_id))
}

/// 「这一处我知道了」：记下这条问题的指纹（幂等，重复点不算错）。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_dismiss(
    data: State<'_, AppData>,
    work_id: i64,
    fingerprint: String,
) -> Result<OutlineBoardDto, ApiError> {
    crate::acceptance::note_command("outline_dismiss");
    data.with_store(|store| {
        store.dismiss_issue(work_id, &fingerprint, "author")?;
        board(store, work_id)
    })
}

/// 撤销一次忽略（回头路）。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_undismiss(
    data: State<'_, AppData>,
    work_id: i64,
    fingerprint: String,
) -> Result<OutlineBoardDto, ApiError> {
    crate::acceptance::note_command("outline_undismiss");
    data.with_store(|store| {
        store.undismiss_issue(work_id, &fingerprint, "author")?;
        board(store, work_id)
    })
}

/// 全部重新看一遍（"我改过设定了，从头再扫给我看"）。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_clear_dismissed(
    data: State<'_, AppData>,
    work_id: i64,
) -> Result<OutlineBoardDto, ApiError> {
    crate::acceptance::note_command("outline_clear_dismissed");
    data.with_store(|store| {
        store.clear_dismissed_issues(work_id, "author")?;
        board(store, work_id)
    })
}

/// 交上来的一格：哪一段的哪一栏写什么（界面把粘进来的那一片拆好格再交上来）。
///
/// 拆格在界面（那儿才看得见"哪一列现在露着、哪一行被折叠了"），**入库前的核对在核心**
/// （稳定码、这一段能不能填、是不是这本书的）——所以这张形状只是搬运，不带任何判断。
#[derive(Debug, Deserialize)]
pub struct OutlineCellDto {
    pub node_id: i64,
    /// 稳定码（`summary` / `pov` / `goal` / `conflict` / `outcome`）
    pub column: String,
    pub value: String,
}

/// 把一片粘进来的格子**一次写进库**，回这本书最新那一屏大纲表。
///
/// 一次事务：一片里有一格落不了，整片都不落（理见核心 `store::outline_paste`）。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_paste_cells(
    data: State<'_, AppData>,
    work_id: i64,
    cells: Vec<OutlineCellDto>,
) -> Result<Vec<OutlineRow>, ApiError> {
    crate::acceptance::note_command("outline_paste_cells");
    let cells: Vec<OutlineCell> = cells
        .into_iter()
        .map(|cell| OutlineCell { node_id: cell.node_id, column: cell.column, value: cell.value })
        .collect();
    data.with_store(|store| store.save_outline_cells(work_id, &cells, "author"))
}

/// 换掉一段的出场人物（**整份覆盖**），回这一段最新那一份名单。
#[tauri::command(rename_all = "snake_case")]
pub fn outline_set_cast(
    data: State<'_, AppData>,
    node_id: i64,
    entity_ids: Vec<i64>,
) -> Result<Vec<CastMember>, ApiError> {
    crate::acceptance::note_command("outline_set_cast");
    data.with_store(|store| store.set_node_cast(node_id, &entity_ids, "author"))
}
