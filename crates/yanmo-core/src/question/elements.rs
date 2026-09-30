//! 写作要素：**这本书此刻长什么样**（纯数据，由存储层读出来，这里不认识 SQL）。
//!
//! 三条线各一样事实：章节（有没有这一章、动笔没有）、伏笔（埋了多久还没收）、
//! 灵感碎片（引力排前头的几条）。规则（`super::generate`）只读这些事实，
//! 引力公式与排序一行都不用改——新要素接进来就是**多一类事实 + 多一条规则**。
//!
//! 事实里**没有一个字的句子**：`body` 是作者自己写下的原文（伏笔正文 / 碎片原文），
//! 会被截断成槽位取值随句子一起渲染；句子的措辞一律住在界面字典里。

use crate::model::ForeshadowState;

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

/// 一条伏笔的事实（纯数据：**埋点已经换算成阅读顺序里的下标**，规则不用再认节点 id）。
///
/// `planted_index` 是 [`WritingElements::chapters`] 里的**下标**（0 起）；
/// `None` = 没记埋点 / 埋点指向这一版章节清单里没有的节点（删了、或者是别人书里的），
/// 也就是**判不了"隔了多少章"**——规则一律跳过它，绝不猜。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeshadowFacts {
    pub id: i64,
    pub body: String,
    pub state: ForeshadowState,
    pub planted_index: Option<usize>,
}

/// 一条灵感碎片的事实（纯数据：**引力总分由既有公式算好**，这里不重新算一遍）。
///
/// `total` 就是 `gravity::inspire` 的 `SparkGravity::total`（重要度 × 热度 × 关联度 ×
/// 新颖度，见 [`crate::gravity::inspire`]）——叩问那条线只**用它排序**，不另立一套口径。
/// `anchors` 是这条碎片自己挂的锚点（`chapter:12` 这种），草稿会把它们照抄进去：
/// 于是"跟着这一章走"（`store::question_select` 的按章优先）对灵感问题同样生效。
#[derive(Debug, Clone, PartialEq)]
pub struct SparkFacts {
    pub fragment_id: i64,
    pub body: String,
    pub total: f64,
    pub anchors: Vec<String>,
}

/// 一本书此刻的写作要素。
///
/// 三个字段都是**清单**（各读各的，互不推导）：`chapters` 按阅读顺序、`foreshadows`
/// 按读库的顺序、`sparks` 按引力降序。三条规则各自只认自己那一份清单。
///
/// ⚠️ 没有 `Eq`：引力总分是 `f64`（`PartialEq` 够用，`Eq` 说不成立）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WritingElements {
    pub chapters: Vec<ChapterFacts>,
    pub foreshadows: Vec<ForeshadowFacts>,
    pub sparks: Vec<SparkFacts>,
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
