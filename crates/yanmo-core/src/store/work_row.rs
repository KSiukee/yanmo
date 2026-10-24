//! `works` 那一行的**读形状**：列清单、原始取值、以及"先在 Rust 侧校验再收下"。
//!
//! 为什么单独成件（与 [`super::work`] 的读写分开）：`works` 这张表**加列只加在末尾**
//! ——列清单、下标元组、逐个 `row.get` 三处必须一一对应，加一列就要同时挪三处。
//! 这三处放在一个文件里，"加列"这件事只在这一个屏幕上发生；读写那些命令式的方法
//! （列表 / 改名 / 软删 / 目标章数）的变化理由与它完全不同。
//!
//! （0.102.0 拆出来：v20 加 `genre` / `tags` 时 `work.rs` 到了 408 行。
//! 与 `collection_row.rs` / `manuscript_row.rs` 同一个形状——读的形状与变的理由分开。）

use crate::error::Result;
use crate::model::{ManuscriptState, Work, WorkKind, WorkLanguage};

/// 作品字段列表（顺序与 [`WorkRow`] 对应）。
///
/// `storyline`（v14）与 `status`（v17）、`genre` / `tags`（v20）都**加在末尾**
/// 而不是插在语义相近的列旁边：这一个列表的下标与 [`WorkRow`] 一一对应，
/// 插在中间就要把后面每一个下标都挪一遍——那是纯手滑的机会
/// （加列只加在末尾，这一条从 v14 起就没破过）。
pub(super) const COLS: &str = "id, kind, title, language, target_words, summary, created_at, \
                                updated_at, opened_at, storyline, status, genre, tags";

/// 一行的原始取值——先取成朴素类型，再**在 Rust 侧校验**（不在 SQL 里猜着读）。
pub(super) type WorkRow = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    String,
    i64,
    i64,
    Option<i64>,
    String,
    String,
    String,
    String,
);

/// 把一行的前 13 列读成 [`WorkRow`]（`shelf` 那几条聚合列排在它后面）。
pub(super) fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
    ))
}

/// 原始取值 → [`Work`]：**认不出的稳定码在这里当场报错**（不猜着读）。
pub(super) fn build(row: WorkRow) -> Result<Work> {
    Ok(Work {
        id: row.0,
        kind: WorkKind::parse(&row.1)?,
        title: row.2,
        language: WorkLanguage::parse(&row.3)?,
        target_words: row.4,
        summary: row.5,
        created_at: row.6,
        updated_at: row.7,
        opened_at: row.8,
        storyline: row.9,
        status: ManuscriptState::parse(&row.10)?,
        genre: row.11,
        tags: row.12,
    })
}
