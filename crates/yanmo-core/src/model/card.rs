//! 问题卡（模型）：一张卡长什么样、新建时要给什么。
//!
//! 卡住在**碎片统一存储**（`fragments` 表，`frag_kind = 'question'`）里——
//! 与事件 / 灵感速记 / 口述段落 / 答案池同表，所以它天生带 `source / created_at /
//! used_count / linked / importance / derived_from` 这一套元数据（元数据口径见碎片统一存储的设计）。
//! 生命周期状态机在 [`super::card_state`]（纯逻辑）；读写与迁移在 [`crate::store`]。
//!
//! `derived_from` 是**溯源**：作者被一个问题勾出灵感时记下的那张灵感卡会指回这张问题卡，
//! 于是"这条灵感是哪个问题勾出来的"永远查得到。

use super::card_state::QuestionState;

/// 问题卡（`fragments` 表里 `frag_kind = 'question'` 的一行）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QuestionCard {
    pub id: i64,
    pub work_id: i64,
    /// 问题本身（模板槽位渲染之后的那句话）
    pub body: String,
    /// 来源：核心自带的问题，还是哪个模块提交的（模块名进这里）
    pub source: String,
    /// 哪条问题模板问出来的；空串＝不是模板产的（作者自己写的 / 模块提交的）
    pub template_key: String,
    pub state: QuestionState,
    /// 要素重要度（0~1）——引力公式的一个乘数
    pub importance: f64,
    /// 被问出来的次数——**新颖度冷却**就靠它（问过越多次，越不该再挤到前面）
    pub used_count: i64,
    /// 上次被问出的时刻（毫秒）；从没问过 = `None`。
    ///
    /// 与 `used_count` 分工：次数管"问过多少回"，时刻管"上次是多久以前"——
    /// 新颖度随时间回收要用它（见 `crate::gravity::novelty`）。
    pub last_asked_at: Option<i64>,
    /// 关联的人物 / 线（JSON 数组文本；灵感关联度用它）
    pub linked: String,
    /// 溯源：这条卡是哪张卡派生出来的（作者「记灵感」勾出的灵感卡会指回问题卡）
    pub derived_from: Option<i64>,
    /// 是不是**系统自动派生**出来的（作者自己顺着灵感再问的不算）。
    ///
    /// 它决定两件事：引力里的派生折扣打不打（只打自动的），以及自动派生链深要不要卡上限。
    pub auto_derived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 新建一张问题卡要给的字段（其余字段由库里给默认）。
#[derive(Debug, Clone)]
pub struct NewQuestionCard {
    pub work_id: i64,
    pub body: String,
    /// 来源标注：核心自带的问题写 `core`，模块提交的写模块名（按模块静音要用）
    pub source: String,
    /// 哪条模板产的；不来自模板就给空串
    pub template_key: String,
    /// 0~1，越界会被拒（不静默夹取）
    pub importance: f64,
    /// 关联锚点（`chapter:12` 这种）：生成候选时靠它**去重**，
    /// 将来"这条问题是拿哪一章问的"也靠它；没有就交空数组。
    pub linked: Vec<String>,
    /// 溯源到哪张卡；不是派生就给 `None`
    pub derived_from: Option<i64>,
    /// 系统自动派生的（`true` 时 `derived_from` 必填，且链深不得超过上限）
    pub auto_derived: bool,
}
