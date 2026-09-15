//! 引力与偏好：**机制层**（纯逻辑，零 IO、零文案）。
//!
//! # 复用方式（复制分离：只取机制，语义自己长）
//!
//! 从一处 MIT 许可的复用内核里只取两个**纯逻辑**模块（来源与哈希登记在仓内资料区，
//! 不进公开仓），方法名与参数形状对齐（将来换源只换实现）：
//! [`crate::gravity::forgetting`]（衰减数学）与 [`crate::gravity::state_machine`]（档位判定）；语义在本项目里自己长
//! （记忆强度 → 问题卡的新颖度与冷却档位）。**不搬**它的 storage / service / retrieval——
//! 研墨的权威存储是作品 / 卷 / 章 / 碎片，搬进来会长出第二套存储。
//! 对齐基准是源仓的跨语言测试向量（见 `tests/gravity_vectors.rs`）。
//!
//! [`crate::gravity::attractor`]（引力公式与排序）与 [`crate::gravity::preference`]
//! （偏好学习闭环）是本项目自写。
//!
//! # 分层
//!
//! ```text
//! forgetting（衰减数学）  state_machine（档位）
//!            \            /
//!             attractor（引力 = 时机 × 重要度 × 新颖度 × 派生折扣 × 模板权重）
//!             preference（作者怎么处置，就怎么教同类模板）
//! ```
//!
//! 业务语义（模板池、从要素生候选）在 [`crate::question`]；读写数据库在 [`crate::store`]。

pub mod attractor;
pub mod forgetting;
pub mod novelty;
pub mod preference;
pub mod state_machine;

pub use attractor::{allowed_auto_derivation, gravity, rank, AttractorParams, Candidate, Gravity};
pub use novelty::{days_between, is_cooled, novelty};
pub use forgetting::{decayed_strength, reinforce, should_sink, ForgettingParams};
pub use preference::{apply, signal_for_action, FeedbackSignal, PreferenceParams, TemplateWeight};
pub use state_machine::{decide_level, retention_days, FragmentLevel, StateMachineParams};
