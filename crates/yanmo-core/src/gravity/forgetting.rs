//! 遗忘曲线（纯逻辑）：**指数衰减 + 提及强化 + 该不该沉下去**。
//!
//! # 血统（复制分离：只取机制，语义自己长）
//!
//! - **机制来源**：一处 MIT 许可的 Rust 记忆内核里的同名纯逻辑模块——`code-copy`。
//!   函数名与参数形状**刻意保持一致**，将来源更新或换源时只换实现、不动调用方
//!   （对齐是"以后能换源"的关键）。**来源与哈希登记在仓内资料区**（不进公开仓：
//!   公开内容里不出现内部路径与内部项目名）。
//! - **语义适配**：`concept-adapt`。源里的「记忆强度」在本项目里长成「问题卡 / 碎片的热度」：
//!   强度随天数衰减 —— 换成问题卡的语言就是**消耗掉的新颖度随时间回收**
//!   （见 [`super::novelty::novelty`]）；跌破层级阈值即"沉下去"——换成**进冷却**。
//! - **许可**：源目录 MIT（宽松 → 严格方向，允许）；本文件随研墨按 AGPL-3.0-or-later 分发。
//! - **对齐基准**：`tests/gravity_vectors.rs` 拿源仓的跨语言测试向量逐例核对（1e-9），
//!   证明这次 port 与源/Python 真值一致——三端各实现一遍机制时，这是唯一可靠的对齐办法。

use super::state_machine::FragmentLevel;

/// 曲线参数。默认值与源一致（向量对齐就靠它）。
///
/// `level_thresholds`：每个档位的"沉下去"阈值——新颖度跌破它就进冷却。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForgettingParams {
    pub decay_rate: f64,
    pub reinforcement_delta: f64,
    pub strength_floor: f64,
    pub level_thresholds: [(FragmentLevel, f64); 3],
}

impl Default for ForgettingParams {
    fn default() -> Self {
        Self {
            decay_rate: 0.1,
            reinforcement_delta: 0.3,
            strength_floor: 0.05,
            level_thresholds: [
                (FragmentLevel::Short, 0.3),
                (FragmentLevel::Mid, 0.5),
                (FragmentLevel::Long, 0.7),
            ],
        }
    }
}

/// 强度随天数指数衰减，下限为 `strength_floor`（**永不为零**：沉下去不等于消失）。
pub fn decayed_strength(strength: f64, days_since: f64, p: &ForgettingParams) -> f64 {
    let v = strength * (-p.decay_rate * days_since.max(0.0)).exp();
    v.max(p.strength_floor)
}

/// 提及即强化，封顶 1.0。
pub fn reinforce(strength: f64, p: &ForgettingParams) -> f64 {
    (strength + p.reinforcement_delta).min(1.0)
}

/// 强度是否跌破该档位的阈值（→ 该沉下去了：进冷却，不是删除）。
pub fn should_sink(strength: f64, level: FragmentLevel, p: &ForgettingParams) -> bool {
    let threshold = p
        .level_thresholds
        .iter()
        .find(|(l, _)| *l == level)
        .map(|(_, t)| *t)
        .unwrap_or(p.strength_floor);
    strength < threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decays_over_time_and_never_reaches_zero() {
        let p = ForgettingParams::default();
        let d1 = decayed_strength(0.8, 10.0, &p);
        assert!(d1 < 0.8 && d1 >= p.strength_floor);
        assert_eq!(decayed_strength(0.8, 1_000.0, &p), p.strength_floor);
    }

    #[test]
    fn reinforce_caps_at_one() {
        assert_eq!(reinforce(0.9, &ForgettingParams::default()), 1.0);
        assert_eq!(reinforce(0.5, &ForgettingParams::default()), 0.8);
    }

    #[test]
    fn sink_threshold_follows_the_level() {
        let p = ForgettingParams::default();
        assert!(should_sink(0.2, FragmentLevel::Short, &p));
        assert!(should_sink(0.2, FragmentLevel::Mid, &p));
        assert!(!should_sink(0.6, FragmentLevel::Mid, &p));
        assert!(should_sink(0.4, FragmentLevel::Long, &p));
    }
}
