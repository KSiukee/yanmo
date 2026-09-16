//! 数据模型：作品、节点树、碎片与问题卡。
//!
//! **研墨是"作品容器"不是"一本书"**；结构是**可变深度节点树**，
//! 「卷/章」只是 `NodeKind` 的取值，**不是表结构**。
//! 问题卡同理：它的**数据字段**（`card`）与**生命周期状态机**（`card_state`）
//! 分成两个文件——两者的变化理由不一样（加一列 vs 加一条迁移）。
//! 碎片的**种类闭集**与"面板读到的一条长什么样"在 `fragment`；
//! 大纲本体的两样承载（设定卡与场景卡四格）在 `entity_card` 与 `scene_card`。

mod answer_target;
mod card;
mod card_state;
mod entity_card;
mod fragment;
mod input_source;
mod node;
mod question_tone;
mod scene_card;
mod side_tab;
mod work;

pub use answer_target::AnswerTarget;
pub use card::{NewQuestionCard, QuestionCard};
pub use card_state::{transition, QuestionState, Transition, TRANSITIONS};
pub use entity_card::{Attribute, EntityCard, EntityKind, NewEntityCard};
// 行映射与「不猜着读」的解析归存储层用：不在公开面上（壳用不到，也不该用）
pub(crate) use entity_card::{into_entity_card, RawEntityCard};
pub use fragment::{Fragment, FragmentCount, FragmentKind};
pub use input_source::InputSource;
pub use node::{ChapterNumbering, NamingStyle, Node, NodeKind};
pub use question_tone::QuestionTone;
pub use scene_card::{SceneField, SceneFields};
pub use side_tab::SideTab;
pub use work::{Work, WorkKind, WorkLanguage};
