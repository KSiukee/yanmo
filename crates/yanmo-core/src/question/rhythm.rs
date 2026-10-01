//! 节奏判据：**明显比别人长的那一章**。
//!
//! ★ **这个判据只有这一处**，两个消费端共用：
//!
//! 1. 叩问的那条规则 `rhythm.chapter_very_long`（`question::generate` 里那条）；
//! 2. 界面上那条章长提醒（"这一章 4700 字，这本书中位 2000 字（2.4 倍）——是故意的吗？"，
//!    读数口在 `crate::store::Store::chapter_lengths`）。
//!
//! **别在这里之外再写一个"特别长"**：同一件事两套判据迟早各说各话
//! （界面上说"偏长"、叩问那边说没超，作者只会觉得软件在瞎猜）。
//!
//! 三条口径：
//!
//! 1. **只跟这本书自己比**：中位数取**这本书**的样本章（见 [`is_sample`]）——
//!    不同书的章长本来就差着量级，拿一个全局阈值去比是瞎比；
//! 2. **样本不足不评**（[`MIN_SAMPLES`]）：两三章说不出"分布"，一律不判
//!    （界面因此不吭声，叩问也不产那条问题）；
//! 3. **只报事实、不评判**：这里只说"它比中位数长出一大截"，不说"你该拆章"——
//!    那是句子的事，而句子在界面字典里（核心零文案）。
//!
//! 倍数（`ratio`）用的是 [`crate::question::RhythmParams::very_long_ratio`] 那个旋钮：
//! 默认 2.0，两条线取的是同一个默认值。

use super::ChapterFacts;

/// 少于这么多章不评：两三章说不出"分布"（与叩问那条"比中位数长出一大截"同一个门槛）。
pub const MIN_SAMPLES: usize = 3;

/// 样本：**写了正文、字数 > 0** 的章才算"这本书的分布"里的一员。
///
/// 口径只写在这一处：分布、逐章判定都从这里取样本——两条线各筛各的话，
/// 中位数会悄悄不一样，而"几倍"这种事实一旦对不上，界面上的字就不可信了。
pub fn is_sample(chapter: &ChapterFacts) -> bool {
    chapter.has_body && chapter.char_count > 0
}

/// 样本清单（顺序照原清单；判据要的是"这本书写过的那些章"）。
pub fn samples(chapters: &[ChapterFacts]) -> Vec<&ChapterFacts> {
    chapters
        .iter()
        .filter(|chapter| is_sample(chapter))
        .collect()
}

/// 这本书的章长分布：中位数 + 样本够不够（**判据的闸门**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Distribution {
    /// 中位数（**上中位数**：偶数个取靠后的那个），样本不足时是 0
    pub median: i64,
    /// 样本够不够：不够 ⇒ **一律不判**（界面那条提醒因此不开口）
    pub judged: bool,
}

/// 按"这本书自己写过的章"算中位数与门槛（样本不足、或中位数为 0 ⇒ `judged = false`）。
///
/// 取**上中位数**（`counts[len / 2]`）而不是平均两个中间值：它落在某一个真实章的字数上，
/// "中位 2000 字"念给作者听时是个真数（平均出来的 1750 谁也没写过）。
///
/// **不判的时候中位数报 0**（不是"某一章的字数"）：免得下游把它当成这本书的分布用。
pub fn distribution(chapters: &[ChapterFacts]) -> Distribution {
    let mut counts: Vec<i64> = samples(chapters).iter().map(|c| c.char_count).collect();
    counts.sort_unstable();
    let median = counts.get(counts.len() / 2).copied().unwrap_or(0);
    let judged = counts.len() >= MIN_SAMPLES && median > 0;
    Distribution {
        median: if judged { median } else { 0 },
        judged,
    }
}

/// 这一章的字符数算不算"比中位数长出一大截"（`> 中位数 × 倍数`）。
///
/// **判据只有这一处**：样本不足 / 中位数为 0 ⇒ 一律不判（返回 `false`）。
/// 比的是**严格大于**：正好等于"中位数 × 倍数"的那一章不算（踩线不报）。
pub fn exceeds(chars: i64, dist: &Distribution, ratio: f64) -> bool {
    dist.judged && chars > (dist.median as f64 * ratio) as i64
}

/// 一章的章长事实（**只报数**：多少字、偏不偏长）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterLength {
    pub node_id: i64,
    pub chars: i64,
    /// 偏长：超过"中位数 × 倍数"（样本不足时一律 `false`）
    pub long: bool,
}

/// 一本书的章长判定：分布 + 逐章事实（顺序照原清单，两次调用结果一样）。
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterLengths {
    pub distribution: Distribution,
    /// 判据用的那个倍数（如实报给界面："超过中位数的两倍"这件事要解释得出来）
    pub ratio: f64,
    /// 样本门槛（"为什么这次没说话"也要解释得出来）
    pub min_samples: usize,
    pub chapters: Vec<ChapterLength>,
}

/// 逐章判一遍（**只读事实、不评判**：谁偏长由调用方决定要不要说、怎么说）。
pub fn judge(chapters: &[ChapterFacts], ratio: f64) -> ChapterLengths {
    let dist = distribution(chapters);
    let rows = chapters
        .iter()
        .map(|chapter| ChapterLength {
            node_id: chapter.node_id,
            chars: chapter.char_count,
            // 只对样本里的章判：没写过正文的章不进分布，也不该被标成"偏长"
            long: is_sample(chapter) && exceeds(chapter.char_count, &dist, ratio),
        })
        .collect();
    ChapterLengths {
        distribution: dist,
        ratio,
        min_samples: MIN_SAMPLES,
        chapters: rows,
    }
}
