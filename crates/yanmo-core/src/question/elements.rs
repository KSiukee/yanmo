//! 写作要素：**这本书此刻长什么样**（纯数据，由存储层读出来，这里不认识 SQL）。
//!
//! 四路事实各一样：章节（有没有这一章、动笔没有）、伏笔（埋了多久还没收）、
//! 灵感碎片（引力排前头的几条）、人物（谁出场过、之后静了多久）。
//! 规则（`super::generate`）只读这些事实，
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

/// 一条伏笔的事实（纯数据：**"隔了多少章"已经算好**，规则不必再认节点 id、也不必自己数）。
///
/// `chapters_since` = 埋点到**最后一章**隔了多少章，**由存储层按"正文单位"那一份清单算**
/// （`Store::text_spine`：承载正文且不是场景卡——与大纲体检那条同阈值判据**同一份坐标**）。
/// 为什么不让机制层自己从 [`WritingElements::chapters`] 里减：那份清单含场景卡，
/// 而"埋了 20 章还没收"数的是章——同一个阈值架在两份清单上，夹了场景卡的两边就会打架
/// （一边说该收了、一边说还没到）。数在哪算、就按哪的口径，这里只收结果。
///
/// `None` = 没记埋点 / 埋点不在那份清单里（删了、或者是别人书里的），
/// 也就是**判不了"隔了多少章"**——规则一律跳过它，绝不猜。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeshadowFacts {
    pub id: i64,
    pub body: String,
    pub state: ForeshadowState,
    pub chapters_since: Option<i64>,
}

/// 一个人物的事实（纯数据：**"静了多少章"已经算好**，规则不必再认节点 id、也不必自己数）。
///
/// `chapters_since` = 这个人**最后一次出场**之后又过了多少章（0 = 最后一章里还出场），
/// **由存储层按"正文单位"那一份清单算**（`Store::text_spine`：承载正文且不是场景卡——
/// 与伏笔那条"隔了多少章"同一份坐标，见 [`ForeshadowFacts`]）。
/// 为什么不让机制层自己从 [`WritingElements::chapters`] 里减：那份清单含场景卡，
/// 而"静了 12 章没露面"数的是章——同一个阈值架在两份清单上，夹了场景卡的两边就会打架。
///
/// `None` = 判不了：**从没在书里出场过**，或者出场的那几个节点已不在正文单位清单里
/// （删了、是场景卡、或者是别人书里的）——规则一律跳过它，绝不猜
/// （"从没出场过"不是"这条线断了"，是"这条线还没起"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterFacts {
    pub id: i64,
    pub name: String,
    pub chapters_since: Option<i64>,
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
/// 四个字段都是**清单**（各读各的，互不推导）：`chapters` 按阅读顺序、`foreshadows`
/// 按读库的顺序、`sparks` 按引力降序、`characters` 按名字排。四条规则各自只认自己那一份清单。
///
/// ⚠️ 没有 `Eq`：引力总分是 `f64`（`PartialEq` 够用，`Eq` 说不成立）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WritingElements {
    pub chapters: Vec<ChapterFacts>,
    pub foreshadows: Vec<ForeshadowFacts>,
    pub sparks: Vec<SparkFacts>,
    /// 人物线：谁出场过、之后静了多久（**派生出来的推进度**，不是作者标出来的）
    pub characters: Vec<CharacterFacts>,
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
