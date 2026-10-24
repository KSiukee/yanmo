//! 模块通信层的**决策引擎**：准入、排队、截止、取消、断路、崩溃预算。
//!
//! # 它不做什么
//!
//! **不发 IPC、不杀进程、不读时钟、不开线程**。它是一台纯内存的状态机：
//! 宿主每发起一次调用前问它 `ModuleComm::try_start`，拿到 `Started` 才去发；
//! 调用结束/失败/崩溃回来 `ModuleComm::complete`；定时调 `ModuleComm::expire`
//! 收超时与晋升。没有 `recv` / `wait` / 阻塞队列——**核心与宿主都不会在这里等**。
//!
//! # 三条铁律怎么落
//!
//! 1. **核心绝不等模块**：所有方法立即返回；写作热路径根本不经过这里
//!    （模块调用由宿主发起，核心的落盘/切章不依赖任何模块）。
//! 2. **一切有界**：在途数、排队数、响应体、崩溃重启次数都有上限；
//!    队列满了返回 `RejectReason::QueueFull`，**明确拒绝**而不是无限堆内存。
//! 3. **一切可取消**：排队中取消立刻移除；在途取消立刻释放名额；超时由
//!    `ModuleComm::expire` 统一扫描——不留孤儿任务（真正的进程回收在宿主）。
//!
//! # 边界
//!
//! - 进程 / 内存 / CPU / Job Object 的硬限制在宿主（Windows 权限内）；
//! - 崩溃后"重启进程"是宿主的动作；这里只裁决 `Restart` 还是 `Stopped`；
//! - 裁决 `Stopped` 后，宿主必须把模块**写进注册表冻结**并告知用户（本层不碰库）；
//! - SQLite 写锁是库级的，长事务会阻塞所有模块——事务超时与强制 rollback 在
//!   数据写入护栏那一层，不在这里。

mod policy;

pub use policy::{
    Admission, CallPlace, CallResult, CallToken, CancelEffect, CommEvent, CommPolicy,
    CompleteEffect, CrashDecision, FailureKind, HealthState, ModuleHealth, RejectReason,
};

use std::collections::{BTreeMap, VecDeque};

use crate::error::Result;

/// 一台按模块分账的通信决策器（**宿主独占**；核心只读不到它）。
#[derive(Debug)]
pub struct ModuleComm {
    policy: CommPolicy,
    next_token: u64,
    modules: BTreeMap<String, ModuleState>,
}

#[derive(Debug, Default)]
struct ModuleState {
    /// 在途令牌 → 截止时间
    in_flight: BTreeMap<CallToken, i64>,
    /// 排队令牌 → 截止时间（从 `try_start` 那一刻算）
    queue: VecDeque<(CallToken, i64)>,
    failures: u32,
    /// 断路器打开到什么时候（0 = 关着）
    open_until: i64,
    crashes: VecDeque<i64>,
    stopped: bool,
}

impl ModuleState {
    fn allows(&self, now: i64) -> bool {
        !self.stopped && (self.open_until == 0 || now >= self.open_until)
    }
}

impl ModuleComm {
    pub fn new(policy: CommPolicy) -> Result<Self> {
        policy.validate()?;
        Ok(Self { policy, next_token: 1, modules: BTreeMap::new() })
    }

    pub fn policy(&self) -> CommPolicy {
        self.policy
    }

    /// 问一次"现在能不能开始"：**立即返回**，不等待任何东西。
    pub fn try_start(&mut self, module_id: &str, now: i64) -> Admission {
        let token = self.mint();
        let policy = self.policy;
        let state = self.modules.entry(module_id.to_string()).or_default();
        if state.stopped {
            return Admission::Rejected(RejectReason::Stopped);
        }
        if !state.allows(now) {
            return Admission::Rejected(RejectReason::CircuitOpen);
        }
        let deadline = now.saturating_add(policy.deadline_ms);
        if state.in_flight.len() < policy.max_in_flight {
            state.in_flight.insert(token, deadline);
            Admission::Started(token)
        } else if state.queue.len() < policy.max_queued {
            state.queue.push_back((token, deadline));
            Admission::Queued(token)
        } else {
            Admission::Rejected(RejectReason::QueueFull)
        }
    }

    /// 宿主报回一次调用的结果（**只报事实**；超时由 deadline 判，不由宿主猜）。
    pub fn complete(
        &mut self,
        module_id: &str,
        token: CallToken,
        result: CallResult,
        now: i64,
    ) -> CompleteEffect {
        let policy = self.policy;
        let Some(state) = self.modules.get_mut(module_id) else {
            return CompleteEffect::not_accepted();
        };
        if state.in_flight.remove(&token).is_none() {
            return CompleteEffect::not_accepted();
        }
        let (failure, crash) = match result {
            CallResult::Ok { bytes } if bytes <= policy.max_response_bytes => {
                state.failures = 0;
                state.open_until = 0;
                (None, None)
            }
            CallResult::Ok { .. } => {
                note_failure(state, now, &policy);
                (Some(FailureKind::Oversize), None)
            }
            CallResult::Failed => {
                note_failure(state, now, &policy);
                (Some(FailureKind::Error), None)
            }
            CallResult::Crashed => {
                note_failure(state, now, &policy);
                (Some(FailureKind::Crashed), Some(crash_in(state, now, &policy)))
            }
        };
        let (next, expired) = promote(state, now, &policy);
        CompleteEffect { accepted: true, failure, next, expired, crash }
    }

    /// 上游取消：排队中直接移除；在途则释放名额并晋升下一个。
    pub fn cancel(&mut self, module_id: &str, token: CallToken, now: i64) -> CancelEffect {
        let policy = self.policy;
        let Some(state) = self.modules.get_mut(module_id) else {
            return CancelEffect { cancelled: false, next: None, expired: Vec::new() };
        };
        if let Some(index) = state.queue.iter().position(|(queued, _)| *queued == token) {
            state.queue.remove(index);
            return CancelEffect { cancelled: true, next: None, expired: Vec::new() };
        }
        if state.in_flight.remove(&token).is_some() {
            let (next, expired) = promote(state, now, &policy);
            return CancelEffect { cancelled: true, next, expired };
        }
        CancelEffect { cancelled: false, next: None, expired: Vec::new() }
    }

    /// 扫一遍超时并晋升排队（宿主按自己的节拍调；建议不超过最小 deadline 的一半）。
    pub fn expire(&mut self, now: i64) -> Vec<CommEvent> {
        let policy = self.policy;
        let ids: Vec<String> = self.modules.keys().cloned().collect();
        let mut events = Vec::new();
        for module_id in ids {
            let state = self.modules.get_mut(&module_id).expect("module id listed above");
            if state.stopped {
                state.queue.clear();
                continue;
            }

            let due: Vec<CallToken> = state
                .in_flight
                .iter()
                .filter(|(_, deadline)| **deadline <= now)
                .map(|(token, _)| *token)
                .collect();
            for token in due {
                state.in_flight.remove(&token);
                note_failure(state, now, &policy);
                events.push(CommEvent::TimedOut {
                    module_id: module_id.clone(),
                    token,
                    place: CallPlace::InFlight,
                });
            }

            let mut index = 0;
            while index < state.queue.len() {
                if state.queue[index].1 <= now {
                    let (token, _) = state.queue.remove(index).expect("index exists");
                    events.push(CommEvent::TimedOut {
                        module_id: module_id.clone(),
                        token,
                        place: CallPlace::Queued,
                    });
                } else {
                    index += 1;
                }
            }

            while state.in_flight.len() < policy.max_in_flight && !state.queue.is_empty() {
                if !state.allows(now) {
                    break;
                }
                let (token, deadline) = state.queue.pop_front().expect("not empty");
                if deadline <= now {
                    events.push(CommEvent::TimedOut {
                        module_id: module_id.clone(),
                        token,
                        place: CallPlace::Queued,
                    });
                } else {
                    state.in_flight.insert(token, deadline);
                    events.push(CommEvent::Started { module_id: module_id.clone(), token });
                }
            }
        }
        events
    }

    /// 进程崩了（宿主在发现进程消失时调用）：裁决重启还是停用。
    ///
    /// 崩溃会清掉在途令牌（那些调用已经没了）；重启时保留排队，停用时连排队一起清。
    pub fn record_crash(&mut self, module_id: &str, now: i64) -> CrashDecision {
        let policy = self.policy;
        let state = self.modules.entry(module_id.to_string()).or_default();
        crash_in(state, now, &policy)
    }

    /// 一个模块此刻的健康读数（不存在 = 健康、全零）。
    pub fn health(&self, module_id: &str, now: i64) -> ModuleHealth {
        let Some(state) = self.modules.get(module_id) else {
            return ModuleHealth {
                state: HealthState::Healthy,
                in_flight: 0,
                queued: 0,
                failures: 0,
                crashes: 0,
            };
        };
        let state_kind = if state.stopped {
            HealthState::Stopped
        } else if state.open_until != 0 && now < state.open_until {
            HealthState::Open
        } else if state.open_until != 0 {
            HealthState::HalfOpen
        } else if state.failures > 0 {
            HealthState::Degraded
        } else {
            HealthState::Healthy
        };
        let crashes = state
            .crashes
            .iter()
            .filter(|at| now.saturating_sub(**at) <= self.policy.crash_window_ms)
            .count() as u32;
        ModuleHealth {
            state: state_kind,
            in_flight: state.in_flight.len(),
            queued: state.queue.len(),
            failures: state.failures,
            crashes,
        }
    }

    fn mint(&mut self) -> CallToken {
        let token = CallToken::new(self.next_token);
        self.next_token = self.next_token.saturating_add(1);
        token
    }
}

fn note_failure(state: &mut ModuleState, now: i64, policy: &CommPolicy) {
    state.failures = state.failures.saturating_add(1);
    if state.failures >= policy.failure_threshold {
        state.open_until = now.saturating_add(policy.open_cooldown_ms);
        state.failures = 0;
    }
}

/// 把一个排队令牌晋升为在途；顺手跳过排队期间就过期的令牌。
fn promote(
    state: &mut ModuleState,
    now: i64,
    policy: &CommPolicy,
) -> (Option<CallToken>, Vec<CallToken>) {
    let mut expired = Vec::new();
    while state.in_flight.len() < policy.max_in_flight && !state.queue.is_empty() {
        if !state.allows(now) {
            break;
        }
        let (token, deadline) = state.queue.pop_front().expect("not empty");
        if deadline <= now {
            expired.push(token);
            continue;
        }
        state.in_flight.insert(token, deadline);
        return (Some(token), expired);
    }
    (None, expired)
}

fn crash_in(state: &mut ModuleState, now: i64, policy: &CommPolicy) -> CrashDecision {
    state.crashes.push_back(now);
    while let Some(oldest) = state.crashes.front() {
        if now.saturating_sub(*oldest) > policy.crash_window_ms {
            state.crashes.pop_front();
        } else {
            break;
        }
    }
    let crashes = state.crashes.len() as u32;
    if crashes > policy.crash_limit {
        state.stopped = true;
        state.queue.clear();
        state.in_flight.clear();
        CrashDecision::Stopped { crashes }
    } else {
        // 进程没了，这些调用已经丢了；重启时保留排队（重启后还能接上）
        state.in_flight.clear();
        CrashDecision::Restart { attempt: crashes }
    }
}
