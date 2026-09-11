//! 关窗请求的**兜底时钟**：界面不回话时，绝不能把窗口锁死。
//!
//! 关窗闸门的前提是"界面活着、能回话"。界面若已经死了（WebView 进程被系统回收、
//! 页面崩了没人重载），拦截关窗就等于把人锁在窗里——**这比丢几个字糟糕得多**。
//!
//! 所以窗口关闭请求有四种归宿，按优先级判断（全部集中在这个状态机里，便于单测）：
//!
//! | 状态 | 含义 | 壳的动作 |
//! |---|---|---|
//! | `Idle` | 没人关窗 | 什么都不做 |
//! | `Requested` | 已通知界面，等它回话 | 期限内等；超时＝**界面已死**，壳自己退出 |
//! | `Handling` | 界面已回话，正在落盘/弹对话框 | 不再计时——**拦住是它故意的** |
//! | `Exiting` | 退出流程已开始 | 什么都不做 |
//!
//! 两个细节是刻意的：
//! - **用户再点一次关窗**：界面没回话时＝"我就是要关"（立刻收场）；界面正在处理时＝忽略
//!   （此时屏幕上多半就是"还有内容没存下去"的对话框，该由对话框里的按钮决定）；
//! - **反复点击不重置期限**：否则一直点就一直关不掉。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 界面必须在这么久内回应关窗请求（正常落盘 ≪ 这个数；超时说明它已经死了）。
pub const ANSWER_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Requested { at: Instant },
    Handling,
    Exiting,
}

/// 一次关窗请求的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    /// 第一次请求：已通知界面，等它回话
    Notified,
    /// 界面没回话，用户又点了一次——用户坚持要关
    Insisted,
    /// 界面正在处理（多半正弹着"存不下去"的对话框）：忽略这次点击
    AlreadyHandling,
    /// 已经在退出流程里了
    AlreadyExiting,
}

#[derive(Debug)]
pub struct ExitWatch {
    phase: Mutex<Phase>,
}

impl Default for ExitWatch {
    fn default() -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
        }
    }
}

impl ExitWatch {
    /// 收到一次关窗请求。
    pub fn request(&self) -> RequestOutcome {
        let mut phase = self.lock();
        match *phase {
            Phase::Idle => {
                *phase = Phase::Requested { at: Instant::now() };
                RequestOutcome::Notified
            }
            Phase::Requested { .. } => RequestOutcome::Insisted,
            Phase::Handling => RequestOutcome::AlreadyHandling,
            Phase::Exiting => RequestOutcome::AlreadyExiting,
        }
    }

    /// 界面回话了（收到关窗通知，开始落盘/弹对话框）。
    pub fn acknowledge(&self) {
        let mut phase = self.lock();
        if matches!(*phase, Phase::Requested { .. }) {
            *phase = Phase::Handling;
        }
    }

    /// 退出流程开始（闸门放行）。
    pub fn mark_exiting(&self) {
        *self.lock() = Phase::Exiting;
    }

    /// 还在等界面回话吗？（等＝需要有人守着期限）
    pub fn waiting(&self) -> bool {
        matches!(*self.lock(), Phase::Requested { .. })
    }

    /// 该由壳自己收场了吗？（界面一直没回话，且期限已过）
    pub fn should_force_exit(&self, now: Instant) -> bool {
        match *self.lock() {
            Phase::Requested { at } => now.duration_since(at) >= ANSWER_DEADLINE,
            _ => false,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 留出余量，免得测试受调度抖动影响。
    fn margin() -> Duration {
        Duration::from_millis(50)
    }

    #[test]
    fn idle_shell_never_forces_anything() {
        let watch = ExitWatch::default();
        assert!(!watch.should_force_exit(Instant::now() + Duration::from_secs(600)));
    }

    #[test]
    fn silent_frontend_gets_a_grace_period_then_the_shell_gives_up() {
        let watch = ExitWatch::default();
        assert_eq!(watch.request(), RequestOutcome::Notified);

        let at = Instant::now();
        assert!(!watch.should_force_exit(at + Duration::from_secs(1)), "期限内要等界面");
        assert!(
            watch.should_force_exit(at + ANSWER_DEADLINE + margin()),
            "界面不回话，期限一到壳必须自己收场（否则窗口关不掉）"
        );
    }

    #[test]
    fn acknowledging_disarms_the_deadline() {
        let watch = ExitWatch::default();
        watch.request();
        watch.acknowledge();
        assert!(
            !watch.should_force_exit(Instant::now() + Duration::from_secs(600)),
            "界面已回话＝拦住是它故意的（比如正弹着对话框），壳不能替它做主"
        );
    }

    #[test]
    fn clicking_close_again_while_silent_means_the_user_insists() {
        let watch = ExitWatch::default();
        assert_eq!(watch.request(), RequestOutcome::Notified);
        assert_eq!(watch.request(), RequestOutcome::Insisted);
    }

    #[test]
    fn clicking_close_again_while_handling_is_ignored() {
        let watch = ExitWatch::default();
        watch.request();
        watch.acknowledge();
        assert_eq!(watch.request(), RequestOutcome::AlreadyHandling);
    }

    #[test]
    fn repeated_clicks_do_not_push_the_deadline_back() {
        let watch = ExitWatch::default();
        let at = Instant::now();
        for _ in 0..10 {
            watch.request();
        }
        assert!(
            watch.should_force_exit(at + ANSWER_DEADLINE + margin()),
            "一直点关窗不该让它一直关不掉"
        );
    }

    #[test]
    fn exiting_is_terminal() {
        let watch = ExitWatch::default();
        watch.request();
        watch.mark_exiting();
        assert_eq!(watch.request(), RequestOutcome::AlreadyExiting);
        assert!(!watch.should_force_exit(Instant::now() + Duration::from_secs(600)));
    }

    #[test]
    fn waiting_is_only_true_while_we_expect_an_answer() {
        let watch = ExitWatch::default();
        assert!(!watch.waiting(), "没人关窗时不该有人守着期限");
        watch.request();
        assert!(watch.waiting(), "已通知界面、还没回话：要看守期限");
        watch.acknowledge();
        assert!(!watch.waiting(), "界面回话后不用再守");
    }
}
