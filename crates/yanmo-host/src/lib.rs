//! 研墨模块沙箱宿主（**独立进程**）。
//!
//! 这个 crate **不链接 `yanmo-core`**，只依赖公开协议层 `yanmo-proto` 与 WASM 运行时
//! `wasmi`。它的存在本身是结构约束的一部分：模块平台可以独立构建 / 独立剥离，
//! 核心与模块之间只有协议口。
//!
//! 能力边界：
//! - 只跑 `manifest.json` 指向的 `.wasm`，包内不许有原生可执行文件 / 符号链接；
//! - 没有 WASI、没有文件系统、没有网络；唯一 import 是 `env.emit`；
//! - fuel 中断死循环、`StoreLimits` 限制内存、输出有界；
//! - 只调 guest 的 `run()` 一个导出。
//!
//! 还没做：真正的 IPC（回环 + 一次性 token）、从核心拉取只读数据、子 WebView、
//! 进程级资源配额（Job Object）。这些在后续刀里接。

#![forbid(unsafe_code)]

pub mod error;
pub mod network;
mod package;
pub mod core_link;
mod sandbox;
mod sandbox_network;
mod sandbox_config;

pub use error::{HostError, HostResult};
pub use package::{
    ModulePackage, MAX_ENTRY_BYTES, MAX_MANIFEST_BYTES, MAX_PACKAGE_BYTES, MAX_PACKAGE_ENTRIES,
};
pub use sandbox::{run_package, run_package_with_core, run_package_with_core_shared, run_package_with_core_shared_cancellable, RunReport, RunStatus};
pub use core_link::CoreLink;
pub use sandbox_config::SandboxConfig;
