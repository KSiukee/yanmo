//! 通信层的**预算与结果类型**：策略、令牌、准入/拒绝原因、完成效果、健康读数。
//!
//! 这些都是**纯数据**，不碰进程、不碰 IO；宿主拿它们做决定。
//! 所有稳定码（`as_str`）写进诊断日志与将来的管理页，**别改已发布的取值**。

use serde::{Deserialize, Serialize};

use crate::error::{codes, Error, Result};

/// 通信预算（**一台引擎一套**；要按模块给不同预算，就按预算类各开一台引擎）。
///
/// 默认值保守：同时 2 个在途、排队 8 个、单请求 5 秒、响应 1 MiB、
/// 连续 3 次失败开断路器、冷却 30 秒、10 分钟内最多重启 2 次。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommPolicy {
    /// 同时在途的请求上限（必须 ≥1）
    pub max_in_flight: usize,
    /// 排队上限（0 = 不排队，满了直接拒）
    pub max_queued: usize,
    /// 单请求的截止时间（毫秒；从 `try_start` 那一刻算，排队也算在里面）
    pub deadline_ms: i64,
    /// 响应体上限（字节；超过按失败计，**不截断冒充成功**）
    pub max_response_bytes: usize,
    /// 连续失败几次开断路器
    pub failure_threshold: u32,
    /// 断路器打开后多久允许试探（毫秒）
    pub open_cooldown_ms: i64,
    /// 崩溃预算：最多自动重启几次
    pub crash_limit: u32,
    /// 崩溃预算的统计窗口（毫秒）
    pub crash_window_ms: i64,
}

impl Default for CommPolicy {
    fn default() -> Self {
        Self {
            max_in_flight: 2,
            max_queued: 8,
            deadline_ms: 5_000,
            max_response_bytes: 1024 * 1024,
            failure_threshold: 3,
            open_cooldown_ms: 30_000,
            crash_limit: 2,
            crash_window_ms: 10 * 60 * 1000,
        }
    }
}

impl CommPolicy {
    /// 校验预算本身（0 在途 / 0 截止 / 0 响应上限 / 0 失败阈值都是写错了）。
    pub fn validate(&self) -> Result<()> {
        if self.max_in_flight == 0 {
            return Err(invalid("max_in_flight must be >= 1"));
        }
        if self.deadline_ms <= 0 {
            return Err(invalid("deadline_ms must be > 0"));
        }
        if self.max_response_bytes == 0 {
            return Err(invalid("max_response_bytes must be > 0"));
        }
        if self.failure_threshold == 0 {
            return Err(invalid("failure_threshold must be >= 1"));
        }
        if self.open_cooldown_ms < 0 {
            return Err(invalid("open_cooldown_ms must be >= 0"));
        }
        if self.crash_window_ms <= 0 {
            return Err(invalid("crash_window_ms must be > 0"));
        }
        Ok(())
    }
}

fn invalid(detail: &str) -> Error {
    Error::invalid_with(codes::MODULE_COMM_POLICY_INVALID, [("detail", detail.to_string())])
}

/// 一次调用的令牌（宿主只拿来对账；不要自己造）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CallToken(u64);

impl CallToken {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

/// 一次准入的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// 立刻可以跑（宿主去发起）
    Started(CallToken),
    /// 排队了（等前面的完成；宿主不要自己再排一层）
    Queued(CallToken),
    /// 明确拒绝（不是错误重试，是"现在就是不行"）
    Rejected(RejectReason),
}

/// 拒绝原因（稳定码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// 断路器打开：快速失败，别等
    CircuitOpen,
    /// 崩溃预算用完，模块已停用
    Stopped,
    /// 队列满了（背压的最后一档）
    QueueFull,
}

impl RejectReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            RejectReason::CircuitOpen => "circuit_open",
            RejectReason::Stopped => "stopped",
            RejectReason::QueueFull => "queue_full",
        }
    }
}

/// 宿主报告的一次调用结果（**只报事实**，不报耗时判断——超时由 broker 按 deadline 判）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallResult {
    Ok { bytes: usize },
    Failed,
    Crashed,
}

/// 一次完成里出现的失败种类（稳定码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// 模块报错 / 协议错
    Error,
    /// 响应超过上限（不截断冒充成功）
    Oversize,
    /// 进程崩了
    Crashed,
}

impl FailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            FailureKind::Error => "error",
            FailureKind::Oversize => "oversize",
            FailureKind::Crashed => "crashed",
        }
    }
}

/// 崩溃预算的裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum CrashDecision {
    /// 允许重启；`attempt` 是窗口内的第几次
    Restart { attempt: u32 },
    /// 预算用完：**停用模块并告知用户**（停用由宿主写注册表）
    Stopped { crashes: u32 },
}

/// 一次完成的效果：失败种类、下一个可以启动的排队令牌、被跳过的过期令牌、崩溃裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteEffect {
    /// 这个令牌是不是真的在途（重复完成 / 伪造令牌 = false，什么都不改）
    pub accepted: bool,
    pub failure: Option<FailureKind>,
    pub next: Option<CallToken>,
    /// 排队期间就过了截止时间、被跳过的令牌（宿主应当按超时告诉他们）
    pub expired: Vec<CallToken>,
    pub crash: Option<CrashDecision>,
}

impl CompleteEffect {
    pub(crate) fn not_accepted() -> Self {
        Self { accepted: false, failure: None, next: None, expired: Vec::new(), crash: None }
    }
}

/// 一次取消的效果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelEffect {
    pub cancelled: bool,
    pub next: Option<CallToken>,
    pub expired: Vec<CallToken>,
}

/// 过期扫描的事件（宿主据此把超时告诉上游、去启动被晋升的调用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CommEvent {
    TimedOut { module_id: String, token: CallToken, place: CallPlace },
    /// 排队晋升为在途：宿主现在去发起
    Started { module_id: String, token: CallToken },
}

/// 超时发生在哪一档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallPlace {
    InFlight,
    Queued,
}

/// 模块健康状态（给诊断 / 管理页看）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    /// 有失败但还没到断路器阈值
    Degraded,
    /// 断路器打开（快速失败中）
    Open,
    /// 冷却结束、允许试探；还没有结果证明它恢复了
    HalfOpen,
    /// 崩溃预算用完，已停用
    Stopped,
}

impl HealthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            HealthState::Healthy => "healthy",
            HealthState::Degraded => "degraded",
            HealthState::Open => "open",
            HealthState::HalfOpen => "half_open",
            HealthState::Stopped => "stopped",
        }
    }
}

/// 一个模块的通信现状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ModuleHealth {
    pub state: HealthState,
    pub in_flight: usize,
    pub queued: usize,
    pub failures: u32,
    pub crashes: u32,
}
