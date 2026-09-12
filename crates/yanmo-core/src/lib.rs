//! 研墨核心引擎：**数据权威 + 全部业务逻辑**。
//!
//! # 设计铁律
//!
//! 1. **零 UI 依赖**：本 crate 不得依赖 Tauri / WebView / 任何壳。它必须能被
//!    单元测试直接驱动，也能被 CLI 或压测脚本直接驱动（300 万字基准就靠这条）。
//! 2. **单一真相源**：所有读写都发生在这里，壳只调用，不碰文件系统。
//! 3. **代码健康**：单文件软上限 200 行、单职责、禁 utils 垃圾桶；
//!    行数只是信号，**重复与耦合才是病**——不要为了凑行数硬拆。
//! 4. **不提供网络接口**：核心只做本机数据处理，对外通道另行设计且默认关闭。

pub mod atomic;
pub mod db;
pub mod error;
pub mod error_codes;
pub mod model;
pub mod store;
pub mod text;
pub mod time;
pub mod version;

pub use error::{Error, Result};
pub use version::{describe, engine_version, VersionInfo};
