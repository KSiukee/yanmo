//! 叩问的**机制侧**：模板池 + 从已有数据生成候选草稿。
//!
//! 分工：这里只认「模板 / 槽位 / 章节事实」这些纯数据，不碰数据库、不产句子
//! （句子在界面字典里按语言渲染——核心零文案）。引力与偏好那套纯机制在
//! [`crate::gravity`]；读写与选题排序在 [`crate::store`]。
//!
//! ```text
//! 模板池（template）──▶ 草稿（generate）──▶ 落成卡（store）──▶ 排序（gravity）──▶ 问出来
//! ```

pub mod defer;
pub mod defer_preset;
pub mod elements;
pub mod push;
pub mod generate;
/// 节奏判据：**明显比别人长的那一章**（叩问那条规则与界面那条章长提醒共用同一个判据）。
pub mod rhythm;
pub mod template;

pub use defer::{anchors_of, chapter_anchor, DeferCondition, DeferKind, DAY_MS};
pub use defer_preset::DeferPreset;
pub use elements::{
    ChapterFacts, CharacterFacts, ForeshadowFacts, LineFacts, RhythmParams, SparkFacts,
    WritingElements,
};
pub use push::{advanced, decide, PushDecision, PushQuota, PushReason, PushState};
pub use generate::{generate, QuestionDraft, STALLED_CAST_CHAPTERS, STALLED_LINE_CHAPTERS};
pub use rhythm::{distribution, exceeds, is_sample, judge, ChapterLength, ChapterLengths, Distribution, MIN_SAMPLES};
pub use template::{
    template, urgency_of, ElementKind, QuestionTemplate, DEFAULT_URGENCY, TEMPLATES,
};
