//! 正文来源账（`node_source_runs`）：**按区间读写「这一段是谁写进来的」**。
//!
//! 写入只有两个时机：
//!
//! 1. **编辑器落盘**：`store::content` 在写正文的同一个事务里，把新旧正文比对的结果
//!    交给 [`super::source_trace`] 算成新的一串段；
//! 2. **直接改正文的路径**（导入、采纳灵感、镜像同步、作答落章）：那些路径没有观测者，
//!    它们会**清掉**这一章的来源账——读不到就是"未记录"，绝不拿旧账冒充新正文。
//!
//! 读只有一处：算「这一章的字从哪来」（[`Store::source_breakdown`]）。
//! 它返回**分档的字数**，三口径都给（与 `ContentStats` 同一套规则，见 `text::tally_by_source`），
//! 各档加起来必须等于正文总数——这是这条账的验收线。
//!
//! # 老库与坏记录
//!
//! 没有这一行 / JSON 坏了：一律当"整篇未记录"（一档 `unknown`），不猜。
//! 这样界面上老稿子显示「未记录」，新写的字才逐笔有来源——如实，而不是看起来漂亮。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::source_run::{decode, encode, normalize, Run};
use super::Store;
use crate::error::Result;
use crate::model::EditOrigin;
use crate::text;
use crate::time::now_millis;

/// 来源账里的一档：某个来源贡献了多少字（三个口径）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceShare {
    /// 稳定码（`typed` / `pasted` / `module` / `voice` / `system` / `mixed` / `unknown`）
    pub source: String,
    pub char_count: i64,
    pub chars_no_punct: i64,
    pub word_count: i64,
}

/// 一章的来源分布。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct SourceBreakdown {
    /// 只含有字的档，按固定顺序（键入 / 粘贴 / 模块 / 口述 / 软件写入 / 混合 / 未记录）。
    pub shares: Vec<SourceShare>,
}

impl SourceBreakdown {
    /// 某一档的字数（找不到＝0）。
    pub fn chars_of(&self, source: EditOrigin) -> i64 {
        self.shares
            .iter()
            .find(|share| share.source == source.as_str())
            .map(|share| share.char_count)
            .unwrap_or(0)
    }
}

/// 正文 + 段 → 分档字数。**段必须铺满正文**（不铺满时按 `unknown` 补齐）。
pub(super) fn breakdown_of(body: &str, runs: &[Run]) -> SourceBreakdown {
    if body.is_empty() {
        return SourceBreakdown::default();
    }
    let runs = normalize(runs, body.len());
    let tallies = text::tally_by_source(body, runs.len(), |at| {
        runs.partition_point(|run| run.start <= at).saturating_sub(1)
    });

    // 按固定顺序归并：同一来源在账里本来已合并，这里再合一次只是防守
    let mut slots = [text::Tally::default(); EditOrigin::ALL.len()];
    for (index, tally) in tallies.iter().enumerate() {
        let origin = runs.get(index).map(|run| run.origin).unwrap_or(EditOrigin::Unknown);
        let slot = &mut slots[origin_index(origin)];
        slot.chars += tally.chars;
        slot.chars_no_punct += tally.chars_no_punct;
        slot.words += tally.words;
    }

    let mut shares = Vec::new();
    for (index, origin) in EditOrigin::ALL.into_iter().enumerate() {
        let tally = slots[index];
        if tally == text::Tally::default() {
            continue;
        }
        shares.push(SourceShare {
            source: origin.as_str().to_string(),
            char_count: tally.chars,
            chars_no_punct: tally.chars_no_punct,
            word_count: tally.words,
        });
    }
    SourceBreakdown { shares }
}

fn origin_index(origin: EditOrigin) -> usize {
    EditOrigin::ALL.iter().position(|known| *known == origin).unwrap_or(EditOrigin::ALL.len() - 1)
}

/// 读这一章的段。没有记录 / 记录坏了 → `None`（＝整篇未记录）。
pub(super) fn read_runs(conn: &Connection, node_id: i64) -> Result<Option<Vec<Run>>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT runs FROM node_source_runs WHERE node_id = ?1",
            params![node_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw.as_deref().and_then(decode))
}

/// 整体替换这一章的段（与正文同一个事务提交）。
pub(super) fn write_runs(conn: &Connection, node_id: i64, runs: &[Run]) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO node_source_runs(node_id, runs, updated_at) VALUES(?1, ?2, ?3)",
        params![node_id, encode(runs), now_millis()],
    )?;
    Ok(())
}

/// 清掉这一章的段：正文从别的路径改了，旧账已经没有意义（读不到＝未记录）。
pub(super) fn clear_runs(conn: &Connection, node_id: i64) -> Result<()> {
    conn.execute("DELETE FROM node_source_runs WHERE node_id = ?1", params![node_id])?;
    Ok(())
}

impl Store {
    /// 这一章的字从哪来（分档字数）。**只读**，不改账。
    pub fn source_breakdown(&self, node_id: i64) -> Result<SourceBreakdown> {
        let body = self.read_body(node_id)?;
        let runs = read_runs(&self.conn, node_id)?.unwrap_or_default();
        Ok(breakdown_of(&body, &runs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: EditOrigin = EditOrigin::Typed;
    const P: EditOrigin = EditOrigin::Pasted;

    fn seg(start: usize, end: usize, origin: EditOrigin) -> Run {
        Run { start, end, origin }
    }

    #[test]
    fn shares_are_ordered_and_add_up_to_the_whole_body() {
        let body = "打出来的十个字。粘进来的一小段。";
        let at = body.find("粘").unwrap();
        let runs = vec![seg(0, at, T), seg(at, body.len(), P)];
        let breakdown = breakdown_of(body, &runs);
        let chars: i64 = breakdown.shares.iter().map(|s| s.char_count).sum();
        let words: i64 = breakdown.shares.iter().map(|s| s.word_count).sum();
        assert_eq!(chars, text::count_chars(body));
        assert_eq!(words, text::count_words(body));
        assert_eq!(breakdown.shares[0].source, "typed", "固定顺序：键入在前");
        assert_eq!(breakdown.shares[1].source, "pasted");
        assert!(breakdown.chars_of(P) > 0);
    }

    #[test]
    fn no_ledger_means_the_whole_chapter_is_unrecorded() {
        let body = "老稿子里的字。";
        let breakdown = breakdown_of(body, &[]);
        assert_eq!(breakdown.shares.len(), 1);
        assert_eq!(breakdown.shares[0].source, "unknown");
        assert_eq!(breakdown.shares[0].char_count, text::count_chars(body));
    }

    #[test]
    fn an_empty_body_has_no_shares() {
        assert!(breakdown_of("", &[]).shares.is_empty());
    }
}
