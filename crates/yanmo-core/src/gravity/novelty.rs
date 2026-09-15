//! 新颖度与冷却：**问过多少回、上次多久以前，现在该不该再冒头**。
//!
//! 配方取自 [`super::forgetting`]（衰减数学）与 [`super::state_machine`]（档位）——
//! 这里只做语义适配：源里的「强度」在本项目是**新颖度**（问一次消耗一截、随时间回收）。
//!
//! 两条口径，产品上都有意义：
//!
//! - **从没问过的永远是最高**（1.0）——"不是永远那几个问题"最硬的那一半；
//! - **问过的回不到满分**（消耗量有下限兜底），但时间越久越接近满分——旧问题会慢慢重生。

use super::attractor::AttractorParams;
use super::forgetting::{decayed_strength, reinforce, should_sink, ForgettingParams};
use super::state_machine::{retention_days, FragmentLevel};

/// 一天的毫秒数（衰减的时间口径）。
const MS_PER_DAY: f64 = 86_400_000.0;

/// 迭代上限：只防手改出来的天文数字（默认曲线 4 次就封顶），不改变任何正常取值的结果。
const MAX_SPENT_STEPS: i64 = 64;

/// 问过 N 次的「新颖度消耗量」＝把源机制里的**提及强化**照跑 N 次（封顶 1.0）。
///
/// 刻意不给它另写一行乘法：用机制自己的函数，将来换源时消耗曲线跟着源一起走。
fn spent_novelty(used_count: i64, p: &ForgettingParams) -> f64 {
    let mut spent = 0.0;
    for _ in 0..used_count.max(1).min(MAX_SPENT_STEPS) {
        spent = reinforce(spent, p);
    }
    spent
}

/// 两个时刻之间隔了几天（负数当 0：时钟回拨不该让卡"未来化"）。
pub fn days_between(from_ms: i64, to_ms: i64) -> f64 {
    (to_ms - from_ms).max(0) as f64 / MS_PER_DAY
}

/// **新颖度**：从没问过 = 1.0（最高，对着"优先用新鲜的"那条）；问过之后按消耗记账、随时间回收。
///
/// 消耗量 = 被问过几次 × [`ForgettingParams::reinforcement_delta`]（封顶 1.0）——
/// 每问一次掉一截；随时间按同一条曲线衰减，于是新颖度**回升**，
/// 但因为 `strength_floor` 兜底，问过的卡永远回不到 1.0。
pub fn novelty(
    used_count: i64,
    last_asked_at: Option<i64>,
    now_ms: i64,
    p: &ForgettingParams,
) -> f64 {
    let Some(asked_at) = last_asked_at else { return 1.0 };
    let spent = spent_novelty(used_count, p);
    1.0 - decayed_strength(spent, days_between(asked_at, now_ms), p)
}

/// 这张卡现在是不是"沉下去了"（该冷却）：还在档位冷却期内，或新颖度跌破该档位的阈值。
pub fn is_cooled(
    novelty: f64,
    level: FragmentLevel,
    days_since_asked: f64,
    p: &AttractorParams,
) -> bool {
    let within_window = retention_days(level, &p.levels)
        .is_some_and(|days| days_since_asked < days as f64);
    within_window || should_sink(novelty, level, &p.forgetting)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_asked_beats_asked_and_novelty_recovers_with_time() {
        let p = ForgettingParams::default();
        let now = 1_000 * 86_400_000;
        assert_eq!(novelty(0, None, now, &p), 1.0, "从没问过的新颖度最高");
        let fresh = novelty(1, Some(now), now, &p);
        let old = novelty(1, Some(now - 30 * 86_400_000), now, &p);
        assert!(fresh < old && old < 1.0, "问过就掉一截，时间过去回升，但回不到 1.0");
        assert!(novelty(4, Some(now), now, &p) < fresh, "问得越多掉得越狠");
    }

}
