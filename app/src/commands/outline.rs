//! 大纲体检命令域：**把对不上的地方列出来**（只报告，不改稿）。
//!
//! 规则全在核心的 `outline`（纯逻辑、零文案）；这里只做两件事：
//! 把发现包成界面认的形状（**带上指纹**，忽略标记认它），以及记下"这一处我知道了"。
//!
//! 三个口径：
//!
//! - **只读**：扫一遍不动一个字节（真正常见的动作是"点开看一眼"）；
//! - **零文案**：给的是规则码 + 参数 + 定位锚点，句子在界面字典（`outline.issue.*`）；
//! - **忽略是作者对自己清单的处置**：记下来（按书分），随时能撤销或全部重看。

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::Store;
use yanmo_core::outline::OutlineIssue;

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
