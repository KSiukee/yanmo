//! 主动问一句（推）的**门槛**：配额与冷却——纯逻辑，不认识数据库，也不认识界面。
//!
//! # 为什么"推"要有门槛
//!
//! 叩问有两条路进来，纪律完全不同：
//!
//! - **拉**（作者自己打开面板）：**完全不限**——他想问多少遍就问多少遍，看多少问题都不算"问过"；
//! - **推**（软件自己冒头）：**有配额、有冷却**——写作最怕被打断，所以"主动开口"这件事
//!   必须是**稀缺**的：默认一天最多 3 次，两次之间还要隔一段（默认 60 分钟）。
//!
//! 配额**只管推**：用完了，面板照样能开、问题照样能翻——那是作者主动要看的，
//! 跟"软件自己凑上来"是两码事（任务里写死的口径）。
//!
//! 「一天」按**作者本地时区的那一天**算（调用方把 `today` 算好传进来，见 `time::local_date`）。

/// 推的门槛（来自作者偏好；`per_day <= 0` ＝**不打扰**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushQuota {
    pub per_day: i64,
    pub cooldown_minutes: i64,
}

/// 今天推了几次、上次是什么时候（存在 `settings` 的键值里，按天记账）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PushState {
    /// 记账那一日（`YYYYMMDD` 的整数；跟 `today` 不一样就说明是新的一天，从头算）。
    pub day: i64,
    pub count: i64,
    /// 上次推的时刻（毫秒；0 = 今天还没推过）。
    pub last_at_ms: i64,
}

/// 该不该推。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushDecision {
    /// 可以推。
    Ask,
    /// 今天的次数用完了（或作者把次数设成 0 = 不打扰）。
    QuotaUsed,
    /// 离上一次太近（冷却）。
    TooSoon,
}

impl PushDecision {
    /// 稳定码（进日志与命令行输出；界面不必显示——**错过一次推是静默的**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            PushDecision::Ask => "ask",
            PushDecision::QuotaUsed => "push.quota_used",
            PushDecision::TooSoon => "push.too_soon",
        }
    }
}

/// 看着今天的账决定推不推。
pub fn decide(state: &PushState, quota: &PushQuota, today: i64, now_ms: i64) -> PushDecision {
    // 0（或负数）就是"别打扰我"——它比别的门槛都优先
    if quota.per_day <= 0 {
        return PushDecision::QuotaUsed;
    }
    // 不是今天的账：今天一次都还没推过
    if state.day != today {
        return PushDecision::Ask;
    }
    if state.count >= quota.per_day {
        return PushDecision::QuotaUsed;
    }
    if state.last_at_ms > 0 && now_ms.saturating_sub(state.last_at_ms) < quota.cooldown_minutes * 60_000
    {
        return PushDecision::TooSoon;
    }
    PushDecision::Ask
}

/// 推成功之后的新账（跨天就从头记）。
pub fn advanced(state: &PushState, today: i64, now_ms: i64) -> PushState {
    let count = if state.day == today { state.count } else { 0 };
    PushState { day: today, count: count + 1, last_at_ms: now_ms }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUOTA: PushQuota = PushQuota { per_day: 3, cooldown_minutes: 60 };
    const DAY: i64 = 2026_09_16;

    #[test]
    fn a_fresh_day_starts_with_a_full_allowance() {
        let state = PushState::default();
        assert_eq!(decide(&state, &QUOTA, DAY, 1_000), PushDecision::Ask);
        // 昨天的账不算今天的
        let yesterday = PushState { day: DAY - 1, count: 3, last_at_ms: 999_999_999_999 };
        assert_eq!(decide(&yesterday, &QUOTA, DAY, 1_000), PushDecision::Ask);
    }

    #[test]
    fn quota_runs_out_and_the_cooldown_holds_it_back() {
        let first = advanced(&PushState::default(), DAY, 1_000_000);
        assert_eq!((first.day, first.count), (DAY, 1));
        // 刚推过：太近
        assert_eq!(decide(&first, &QUOTA, DAY, 1_000_000 + 59 * 60_000), PushDecision::TooSoon);
        // 过了冷却：可以再推
        assert_eq!(decide(&first, &QUOTA, DAY, 1_000_000 + 60 * 60_000), PushDecision::Ask);

        // 推满三次：配额用完（冷却过了也没用）
        let mut state = PushState::default();
        for at in [0_i64, 3_600_000, 7_200_000] {
            assert_eq!(decide(&state, &QUOTA, DAY, at), PushDecision::Ask);
            state = advanced(&state, DAY, at);
        }
        assert_eq!(state.count, 3);
        assert_eq!(decide(&state, &QUOTA, DAY, 10_800_000), PushDecision::QuotaUsed);
    }

    #[test]
    fn zero_a_day_means_do_not_disturb() {
        let quiet = PushQuota { per_day: 0, cooldown_minutes: 60 };
        assert_eq!(decide(&PushState::default(), &quiet, DAY, 1_000), PushDecision::QuotaUsed);
    }
}
