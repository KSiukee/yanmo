//! 章长那一屏（**只读**）：这本书每一章多少字、哪一章算"偏长"、哪一章作者已经说过"就这样写"。
//!
//! 三件事：
//!
//! 1. **判据不在这里**：中位数、样本门槛、倍数比对都在 [`crate::question::rhythm`]
//!    （叩问那条 `rhythm.chapter_very_long` 与界面那条章长提醒**共用同一个判据**）；
//!    这里只把书里的事实读出来、拼成界面认的形状；
//! 2. **不新增任何存储**："就这样写"沿用既有的忽略记账（`super::outline_dismiss` 那一族，
//!    与大纲体检同一把记号、按书分）——不建表、不动迁移；
//! 3. **零文案**：给的是一串事实（多少字 / 偏不偏长 / 记过号没有 / 记号原文），句子在界面字典里。
//!
//! 界面拿它做两件事（见 `app/frontend/src/editor/chapter-length.ts`）：
//! 目录里给偏长的章一个安静角标；作者要停下来的那一刻念一句事实（末尾是问句，不是命令）。

use serde::Serialize;

use super::Store;
use crate::error::Result;
use crate::question::rhythm::{self, ChapterLengths};
use crate::question::RhythmParams;

/// 「就这样写」记在忽略记账里的记号（`chapter-long:<节点 id>`）。
///
/// 为什么带规则前缀：那把记号是**按书**存的、与大纲体检共用同一份清单
/// （见 `super::outline_dismiss`），不带前缀的话两条规则会互相认领对方的记号。
pub fn dismiss_identity(node_id: i64) -> String {
    format!("chapter-long:{node_id}")
}

/// 界面要的那一屏：判据那一份事实 + 逐章的"作者说过就这样写没有"。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterLengthBoard {
    /// 中位数（"这本书中位 {median} 字"要念出来）
    pub median: i64,
    /// 判据用的倍数（超过中位数的几倍算特别长）
    pub ratio: f64,
    /// 样本门槛（少于这么多章不评）
    pub min_samples: usize,
    /// 样本够不够：不够 ⇒ 每一章的 `long` 都是 `false`（界面因此不吭声）
    pub judged: bool,
    pub chapters: Vec<ChapterLength>,
}

/// 一章的一行事实。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterLength {
    pub node_id: i64,
    /// 这一章的字符数（与判据同一口径：`nodes.char_count`）
    pub chars: i64,
    /// 偏长：超过"中位数 × 倍数"（样本不足时一律 `false`）
    pub long: bool,
    /// 作者说过「就这样写」——这一章以后不再提
    pub dismissed: bool,
    /// 那条记号的**原文**：界面要写就用它照抄（identity 的写法只有这一处，界面不自己拼）
    pub fingerprint: String,
}

impl Store {
    /// 这本书的章长分布与逐章判定（**只读**：扫一遍不动一个字节，也不新增任何存储）。
    pub fn chapter_lengths(&self, work_id: i64) -> Result<ChapterLengthBoard> {
        // 章节事实与叩问那五路事实**同一份**（`chapter_facts`）：样本口径不各筛各的
        let chapters = self.chapter_facts(work_id)?;
        let lengths = rhythm::judge(&chapters, RhythmParams::default().very_long_ratio);
        Ok(board(lengths, &self.dismissed_issues(work_id)?))
    }
}

/// 判据那一份 + 忽略记账 → 界面那一屏（记号与"记过没有"在这里才合上）。
fn board(lengths: ChapterLengths, dismissed: &[String]) -> ChapterLengthBoard {
    let chapters = lengths
        .chapters
        .into_iter()
        .map(|item| {
            let fingerprint = dismiss_identity(item.node_id);
            ChapterLength {
                node_id: item.node_id,
                chars: item.chars,
                long: item.long,
                dismissed: dismissed.iter().any(|mark| *mark == fingerprint),
                fingerprint,
            }
        })
        .collect();
    ChapterLengthBoard {
        median: lengths.distribution.median,
        ratio: lengths.ratio,
        min_samples: lengths.min_samples,
        judged: lengths.distribution.judged,
        chapters,
    }
}
