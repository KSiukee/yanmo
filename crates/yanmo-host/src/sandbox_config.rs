//! 沙箱预算：**一次运行能用多少 fuel / 内存 / emit / read**。
//!
//! 单独成件：这份配置是宿主与验收共用的公开契约，和沙箱实现分开改。

use crate::error::{HostError, HostResult};

/// 一次沙箱运行的预算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxConfig {
    /// 指令燃料（越大越晚停）
    pub fuel: u64,
    /// 线性内存上限（字节）
    pub max_memory_bytes: usize,
    /// 单条 `emit` 文本上限（字节）
    pub max_emit_bytes: usize,
    /// 全部 `emit` 文本合计上限（字节）
    pub max_total_emit_bytes: usize,
    /// 最多允许几条 `emit`
    pub max_emits: usize,
    /// 单次 `read` 参数（整数 id 文本）上限
    pub max_read_arg_bytes: usize,
    /// 单次 `read` 结果上限（核心侧还有 1 MiB 帧上限）
    pub max_read_result_bytes: usize,
    /// 最多允许几次 `read`
    pub max_reads: usize,
    /// 本次运行是否允许 `env.http_get`（模块声明 + 用户授予 + 全局开关之后）
    pub allow_network: bool,
    /// 本次运行最多几次网络请求
    pub max_network_requests: usize,
    /// 单次网络响应正文上限（字节）
    pub max_network_bytes: usize,
    /// 一次运行累计网络正文上限（字节）
    pub max_total_network_bytes: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            fuel: 10_000_000,
            max_memory_bytes: 16 * 1024 * 1024,
            max_emit_bytes: 64 * 1024,
            max_total_emit_bytes: 256 * 1024,
            max_emits: 64,
            max_read_arg_bytes: 4 * 1024,
            max_read_result_bytes: 256 * 1024,
            max_reads: 64,
            allow_network: false,
            max_network_requests: 8,
            max_network_bytes: 512 * 1024,
            max_total_network_bytes: 2 * 1024 * 1024,
        }
    }
}

impl SandboxConfig {
    pub fn validate(&self) -> HostResult<()> {
        let valid = self.fuel > 0
            && self.max_memory_bytes > 0
            && self.max_emit_bytes > 0
            && self.max_emits > 0
            && self.max_total_emit_bytes >= self.max_emit_bytes
            && self.max_read_arg_bytes > 0
            && self.max_read_result_bytes > 0
            && self.max_reads > 0
            && (!self.allow_network
                || (self.max_network_requests > 0 && self.max_network_bytes > 0));
        if valid {
            Ok(())
        } else {
            Err(HostError::new(HostError::CONFIG, "sandbox limits must be non-zero and consistent"))
        }
    }
}
