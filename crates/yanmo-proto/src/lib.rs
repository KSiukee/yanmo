//! 研墨本机协议（`yanmo-proto`）。
//!
//! # 这个 crate 存在的意义
//!
//! **进程边界 = 许可边界。** 核心通过"公开、稳定、易解析"的协议对外提供服务，
//! 于是：
//! - 第三方可以用任何语言写前端壳（Android / macOS / Linux / Web），**不受 AGPL 约束**；
//! - 我们自己的闭源增值模块也走同一协议，从而与 AGPL 核心合法共存。
//!
//! # 当前阶段的范围
//!
//! **只设计、不实现**。本 crate 目前只承载协议的类型骨架与版本常量；
//! `yanmo-daemon`（把核心包成可独立运行的本机服务）排在 P1。
//!
//! # 已定的协议原则
//!
//! 1. **传输**：首选本机 HTTP（仅绑 `127.0.0.1`）+ JSON + OpenAPI 文档
//!    —— 任何语言都好实现，**AI 也最容易照着写壳**，且能用 `curl` 手工调试。
//! 2. **鉴权**：一次性 token（防止本机其他程序或浏览器页面偷调）。
//! 3. **事件推送**：SSE 或 WebSocket（语音转写回调、长任务进度）。
//! 4. **粒度**：只提供**整章级 / 批量级**接口，**不提供逐字符 API**
//!    —— 否则等于鼓励别人做出卡顿的壳。
//! 5. **版本**：从 `v1` 起，带兼容性声明。

#![forbid(unsafe_code)]

/// 协议版本。破坏性变更才递增，且必须同步更新公开文档。
pub const PROTOCOL_VERSION: u32 = 1;

/// 协议默认监听地址：**只绑回环**，绝不对外暴露。
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_starts_at_v1() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn bind_is_loopback_only() {
        assert_eq!(DEFAULT_BIND_ADDR, "127.0.0.1");
    }
}
