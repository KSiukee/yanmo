//! 数据模型：作品、节点树与问题卡。
//!
//! **研墨是"作品容器"不是"一本书"**；结构是**可变深度节点树**，
//! 「卷/章」只是 `NodeKind` 的取值，**不是表结构**。
//! 问题卡同理：它的**数据字段**（`card`）与**生命周期状态机**（`card_state`）
//! 分成两个文件——两者的变化理由不一样（加一列 vs 加一条迁移）。

mod card;
mod card_state;
mod input_source;
mod node;
mod work;

pub use card::{NewQuestionCard, QuestionCard};
pub use card_state::{transition, QuestionState, Transition, TRANSITIONS};
pub use input_source::InputSource;
pub use node::{ChapterNumbering, NamingStyle, Node, NodeKind};
pub use work::{Work, WorkKind, WorkLanguage};
