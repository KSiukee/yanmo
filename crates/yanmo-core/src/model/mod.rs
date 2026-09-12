//! 数据模型：作品与节点树。
//!
//! **研墨是"作品容器"不是"一本书"**；结构是**可变深度节点树**，
//! 「卷/章」只是 `NodeKind` 的取值，**不是表结构**。

mod node;
mod work;

pub use node::{Node, NodeKind};
pub use work::{Work, WorkKind, WorkLanguage};
