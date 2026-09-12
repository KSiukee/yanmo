//! 版本快照命令：列 / 比 / 留 / 删 / 回滚。
//!
//! 与别的域一样，命令只做「参数转换 + 转交核心」：**滚动保留的边界、回滚先留底的顺序、
//! 手动版本不参与滚动**全在核心那一份实现里（`store::snapshot`），界面照着做就行。
//!
//! 差异是**结构化**的（每行给 kind + 文本 + 行号），红绿怎么画是界面的事——
//! 核心不产出界面文案（见 `lib.rs` 的第 1 条铁律）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::diff::DiffKind;
use yanmo_core::store::{SnapshotSummary, Store};
use yanmo_core::text;

/// 差异行前后各留几句相同的上下文（再远就折起来）。
const DIFF_CONTEXT: usize = 3;

/// 一条快照的摘要（**正文不在这里**——要比对时才拉）。
#[derive(Debug, Serialize)]
pub struct SnapshotDto {
    pub id: i64,
    pub char_count: i64,
    /// close / stuck / desync / keep / before_restore（界面查自己的字典渲染）
    pub reason: String,
    /// 作者亲手留的版本（不参与滚动删除）
    pub pinned: bool,
    pub created_at: i64,
}

fn to_dto(entry: SnapshotSummary) -> SnapshotDto {
    SnapshotDto {
        id: entry.id,
        char_count: entry.char_count,
        reason: entry.reason,
        pinned: entry.pinned,
        created_at: entry.created_at,
    }
}

/// 差异里的一行。
#[derive(Debug, Serialize)]
pub struct DiffLineDto {
    /// same / added / removed / skipped
    pub kind: &'static str,
    pub text: String,
    /// 在旧版（快照）里的行号；新增行为 null
    pub old_line: Option<usize>,
    /// 在当前稿里的行号；删除行为 null
    pub new_line: Option<usize>,
    /// 折叠掉的相同行数（只有 skipped 非 0）
    pub hidden: usize,
}

/// 某条快照与当前正文的比对结果。
#[derive(Debug, Serialize)]
pub struct DiffDto {
    pub lines: Vec<DiffLineDto>,
    pub added: usize,
    pub removed: usize,
    /// 块太大，没有逐行对齐（界面要说清"只给到这一层"）
    pub truncated: bool,
    /// 当前稿多少字（差异视图另一侧的口径）
    pub current_char_count: i64,
}

/// 一次回滚的回执：**回滚后的正文**（界面据此换掉编辑器内容并重建落盘基准）。
#[derive(Debug, Serialize)]
pub struct RestoreAck {
    pub node_id: i64,
    pub body: String,
    pub char_count: i64,
    pub chars_no_punct: i64,
    pub word_count: i64,
    pub fingerprint: String,
}

fn kind_name(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Same => "same",
        DiffKind::Added => "added",
        DiffKind::Removed => "removed",
        DiffKind::Skipped => "skipped",
    }
}

/// 这一章有哪些版本（新的在前）。
#[tauri::command(rename_all = "snake_case")]
pub fn snapshot_list(data: State<'_, AppData>, node_id: i64) -> Result<Vec<SnapshotDto>, ApiError> {
    data.with_store(|store: &mut Store| Ok(store.list_snapshots(node_id)?.into_iter().map(to_dto).collect()))
}

/// 某条快照和**当前正文**差在哪（行级统一差异）。
#[tauri::command(rename_all = "snake_case")]
pub fn snapshot_diff(
    data: State<'_, AppData>,
    node_id: i64,
    snapshot_id: i64,
) -> Result<DiffDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let previous = store.snapshot_body(snapshot_id)?;
        let current = store.read_body(node_id)?;
        let report = yanmo_core::diff::diff_lines(&previous, &current, DIFF_CONTEXT);
        Ok(DiffDto {
            lines: report
                .lines
                .into_iter()
                .map(|row| DiffLineDto {
                    kind: kind_name(row.kind),
                    text: row.text,
                    old_line: row.old_line,
                    new_line: row.new_line,
                    hidden: row.hidden,
                })
                .collect(),
            added: report.added,
            removed: report.removed,
            truncated: report.truncated,
            current_char_count: text::count_chars(&current),
        })
    })
}

/// **手动留一版**：内容与最新一份相同就什么都不做（返回 null），否则返回新快照 id。
#[tauri::command(rename_all = "snake_case")]
pub fn snapshot_keep(data: State<'_, AppData>, node_id: i64) -> Result<Option<i64>, ApiError> {
    data.with_store(|store: &mut Store| store.keep_snapshot(node_id))
}

/// 删掉一条快照（正文一个字都不动）。
#[tauri::command(rename_all = "snake_case")]
pub fn snapshot_drop(data: State<'_, AppData>, snapshot_id: i64) -> Result<(), ApiError> {
    data.with_store(|store: &mut Store| store.drop_snapshot(snapshot_id))
}

/// **回滚**到某一条快照：核心会先把当前这一版留底，再改正文。
#[tauri::command(rename_all = "snake_case")]
pub fn snapshot_restore(data: State<'_, AppData>, snapshot_id: i64) -> Result<RestoreAck, ApiError> {
    data.with_store(|store: &mut Store| {
        let (node_id, body, stats) = store.restore_snapshot(snapshot_id)?;
        Ok(RestoreAck {
            node_id,
            fingerprint: text::content_hash(&body),
            body,
            char_count: stats.char_count,
            chars_no_punct: stats.chars_no_punct,
            word_count: stats.word_count,
        })
    })
}
