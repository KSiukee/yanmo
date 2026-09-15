//! 写作要素：**这本书此刻长什么样**（纯数据，由存储层读出来，这里不认识 SQL）。
//!
//! L1 只用章节这一样；伏笔超期 / 五线推进度 / 灵感碎片引力这些要素接进来时，
//! 在这一层加字段即可——规则（`super::generate`）与引力公式都不必改形状以外的任何东西。

/// 一章的事实（纯数据，由存储层读出来；这里不认识 SQL）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterFacts {
    pub node_id: i64,
    pub title: String,
    /// 动笔了没有（有正文）
    pub has_body: bool,
    pub char_count: i64,
    /// 每章一句话（计划要点）
    pub summary: String,
}

/// 一本书此刻的写作要素（L1 只用这一样：有序的章）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WritingElements {
    pub chapters: Vec<ChapterFacts>,
}

/// 节奏判据的旋钮（集中放，好调）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RhythmParams {
    /// 看最近多少章
    pub swing_window: usize,
    /// 字数极差 / 均值 小于它就认为"雷同"
    pub swing_spread: f64,
    /// 超过中位数的几倍算"特别长"
    pub very_long_ratio: f64,
}

impl Default for RhythmParams {
    fn default() -> Self {
        Self { swing_window: 3, swing_spread: 0.15, very_long_ratio: 2.0 }
    }
}
