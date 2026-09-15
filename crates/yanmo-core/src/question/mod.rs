//! 叩问的**机制侧**：模板池 + 从已有数据生成候选草稿。
//!
//! 分工：这里只认「模板 / 槽位 / 章节事实」这些纯数据，不碰数据库、不产句子
//! （句子在界面字典里按语言渲染——核心零文案）。引力与偏好那套纯机制在
//! [`crate::gravity`]；读写与选题排序在 [`crate::store`]。
//!
//! ```text
//! 模板池（template）──▶ 草稿（generate）──▶ 落成卡（store）──▶ 排序（gravity）──▶ 问出来
//! ```

pub mod elements;
pub mod generate;
pub mod template;

pub use elements::{ChapterFacts, RhythmParams, WritingElements};
pub use generate::{generate, QuestionDraft};
pub use template::{
    template, urgency_of, ElementKind, QuestionTemplate, DEFAULT_URGENCY, TEMPLATES,
};
