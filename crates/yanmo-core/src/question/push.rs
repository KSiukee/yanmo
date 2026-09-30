//! 主动问一句（推）的**门槛**：判据 + 配额与冷却——纯逻辑，不认识数据库，也不认识界面。
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
//! # 先判"这句话成不成立"，再判"配额还够不够"
//!
//! 三个时机里有两句在**空章**里根本不成立："你卡住了"（什么都没写，谈不上卡）、
//! "这一章写完了"（还一个字没有）。这种时候不是"配额省一次"，而是**那句话是假的**——
//! 所以它排在配额前面判，结果也给一个自己的码（[`PushDecision::EmptyChapter`]）。
//!
//! 只有"这一章从哪儿开始"（[`PushReason::NewChapter`]）**本来就是为这一章还空着才问的**，
//! 空章照问。
//!
//! 「一天」按**作者本地时区的那一天**算（调用方把 `today` 算好传进来，见 `time::local_date`）。

/// 为什么现在开口——**核心的枚举**（这三个时机是机制的事，不是界面的事）。
///
/// 界面只把"发生了什么"报上来，判据在核心：空章里 `Idle` / `ChapterDone` 会被拦下
/// （见 [`decide`]）。进 op-log 的 trigger 用它，事后一眼看得出这一次是哪条时机触发的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushReason {
    /// 到了一章还没写字的地方（**空章也问**：它本来就是为这一章空着才问的）。
    NewChapter,
    /// 写着写着停住了（连续一段时间没有动静）。
    Idle,
    /// 刚写完一章、正要走开。
    ChapterDone,
}

impl PushReason {
    /// 稳定码（进 op-log 的 trigger；**与界面传上来的字符串同一个写法**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            PushReason::NewChapter => "new_chapter",
            PushReason::Idle => "idle",
            PushReason::ChapterDone => "chapter_done",
        }
    }

    /// 认界面（或命令行）传上来的字符串。
    ///
    /// **认不出的一律按 [`PushReason::Idle`]**——不静默当"开新章"：`NewChapter` 是唯一
    /// 在空章里也能推的那一个，认错方向会让"空章不问"这条判据形同虚设；
    /// 按 `Idle` 处理最坏是少问一次（错过一次推本来就是静默的）。
    pub fn parse(text: &str) -> PushReason {
        match text {
            "new_chapter" => PushReason::NewChapter,
            "chapter_done" => PushReason::ChapterDone,
            _ => PushReason::Idle,
        }
    }
}
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
    /// **这一章还是空的，而这条时机在空章里不成立**（"你卡住了" / "这一章写完了"）。
    ///
    /// 与配额无关：不是"省一次"，是那句话本身是假的。它与别的"不推"一样**静默**——
    /// 错过的推本来就不该让作者知道；码只进日志与命令行输出。
    EmptyChapter,
}

impl PushDecision {
    /// 稳定码（进日志与命令行输出；界面不必显示——**错过一次推是静默的**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            PushDecision::Ask => "ask",
            PushDecision::QuotaUsed => "push.quota_used",
            PushDecision::TooSoon => "push.too_soon",
            PushDecision::EmptyChapter => "push.empty_chapter",
        }
    }
}

/// 看着今天的账决定推不推。
///
/// `reason` 是为什么现在开口；`chapter_has_body` 是**当前这一章有没有正文**
/// （调用方从库里读出来传进来，见 `store::question_push`）。判序：
///
/// 1. **这句话在这一章里成不成立**（[`PushDecision::EmptyChapter`]）——空章里
///    `Idle` / `ChapterDone` 直接不推；`NewChapter` 本来就是为这一章空着才问的，照旧往下走。
///    它排在配额前面，因为这是**判据**不是资源：日志里也就能一眼分开"没得问"与"问完了"；
/// 2. 配额（`per_day <= 0` 就是"别打扰我"）；
/// 3. 今天的账、冷却。
pub fn decide(
    state: &PushState,
    quota: &PushQuota,
    today: i64,
    now_ms: i64,
    reason: PushReason,
    chapter_has_body: bool,
) -> PushDecision {
    if !chapter_has_body && matches!(reason, PushReason::Idle | PushReason::ChapterDone) {
        return PushDecision::EmptyChapter;
    }
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

    /// 这些用例谈的是配额与冷却，所以一律按"非空的章 + 开新章"喂进判据
    /// （空章那条判据另有专门的用例，见 `an_empty_chapter_is_not_stuck_and_not_done`）。
    fn fresh_day(state: &PushState, today: i64, now_ms: i64) -> PushDecision {
        decide(state, &QUOTA, today, now_ms, PushReason::NewChapter, true)
    }

    #[test]
    fn a_fresh_day_starts_with_a_full_allowance() {
        let state = PushState::default();
        assert_eq!(fresh_day(&state, DAY, 1_000), PushDecision::Ask);
        // 昨天的账不算今天的
        let yesterday = PushState { day: DAY - 1, count: 3, last_at_ms: 999_999_999_999 };
        assert_eq!(fresh_day(&yesterday, DAY, 1_000), PushDecision::Ask);
    }

    #[test]
    fn quota_runs_out_and_the_cooldown_holds_it_back() {
        let first = advanced(&PushState::default(), DAY, 1_000_000);
        assert_eq!((first.day, first.count), (DAY, 1));
        // 刚推过：太近
        assert_eq!(fresh_day(&first, DAY, 1_000_000 + 59 * 60_000), PushDecision::TooSoon);
        // 过了冷却：可以再推
        assert_eq!(fresh_day(&first, DAY, 1_000_000 + 60 * 60_000), PushDecision::Ask);

        // 推满三次：配额用完（冷却过了也没用）
        let mut state = PushState::default();
        for at in [0_i64, 3_600_000, 7_200_000] {
            assert_eq!(fresh_day(&state, DAY, at), PushDecision::Ask);
            state = advanced(&state, DAY, at);
        }
        assert_eq!(state.count, 3);
        assert_eq!(fresh_day(&state, DAY, 10_800_000), PushDecision::QuotaUsed);
    }

    #[test]
    fn zero_a_day_means_do_not_disturb() {
        let quiet = PushQuota { per_day: 0, cooldown_minutes: 60 };
        let got = decide(&PushState::default(), &quiet, DAY, 1_000, PushReason::NewChapter, true);
        assert_eq!(got, PushDecision::QuotaUsed);
    }

    /// 空章判据：**"你卡住了"与"这一章写完了"在空章里根本不成立**，一句话都不该问；
    /// 但"这一章从哪儿开始"本来就是为这一章空着才问的，照旧。
    #[test]
    fn an_empty_chapter_is_not_stuck_and_not_done() {
        let state = PushState::default();
        let ask = |reason, has_body| decide(&state, &QUOTA, DAY, 1_000, reason, has_body);

        assert_eq!(ask(PushReason::Idle, false), PushDecision::EmptyChapter);
        assert_eq!(ask(PushReason::ChapterDone, false), PushDecision::EmptyChapter);
        assert_eq!(ask(PushReason::NewChapter, false), PushDecision::Ask);

        // 非空的章：三条时机照旧
        for reason in [PushReason::NewChapter, PushReason::Idle, PushReason::ChapterDone] {
            assert_eq!(ask(reason, true), PushDecision::Ask, "{reason:?} 在非空章里该照问");
        }
    }

    /// 空章排在配额前面：它是**判据**不是资源——日志里得能一眼分开
    /// "这话在空章里不成立"与"今天问完了"。
    #[test]
    fn the_empty_chapter_verdict_comes_before_the_quota() {
        let quiet = PushQuota { per_day: 0, cooldown_minutes: 0 };
        let state = PushState { day: DAY, count: 9, last_at_ms: 1 };
        let got = decide(&state, &quiet, DAY, 1_000, PushReason::Idle, false);
        assert_eq!(got, PushDecision::EmptyChapter);
        assert_eq!(got.as_str(), "push.empty_chapter");
        assert_eq!(PushReason::Idle.as_str(), "idle");
    }

    /// 认字符串：三个时机认得出来，**认不出的一律按 Idle**（不静默当"开新章"，
    /// 否则空章不问那条判据会被一个手滑的字符串绕过去）。
    #[test]
    fn reasons_are_parsed_and_anything_unknown_is_idle() {
        assert_eq!(PushReason::parse("new_chapter"), PushReason::NewChapter);
        assert_eq!(PushReason::parse("chapter_done"), PushReason::ChapterDone);
        assert_eq!(PushReason::parse("idle"), PushReason::Idle);
        assert_eq!(PushReason::parse("cli"), PushReason::Idle);
        assert_eq!(PushReason::parse(""), PushReason::Idle);
        assert_eq!(PushReason::parse("NEW_CHAPTER"), PushReason::Idle);
    }
}
