//! 层级状态机（纯逻辑）：**按提及次数 + 重要度决定档位**。
//!
//! # 血统（复制分离：只取机制，语义自己长）
//!
//! - **机制来源**：一处 MIT 许可的 Rust 记忆内核里的同名纯逻辑模块——`code-copy`。
//!   `decide_level` / `retention_days` 的签名与判定顺序与源一致（**来源与哈希登记在仓内资料区**，
//!   不进公开仓）；源里那套"提及窗口"在本项目用不上（我们数的是**被问过几次**，
//!   不需要滑动窗口），故未复制该字段——这正是 `concept-adapt` 的分寸：只取用得上的机制。
//! - **语义适配**：源里的「记忆层级」在此长成「**问题卡的冷却档位**」——
//!   被问得越多（提及多）或越重要（重要度高），档位越高：冷却期越长、越不容易再冒头。
//!   对上「两档冷却」的产品口径：短冷却（几章后可能以低权回来）／长冷却（几乎不再出现）。
//! - **许可**：源目录 MIT（宽松 → 严格方向，允许）；本文件随研墨按 AGPL-3.0-or-later 分发。
//! - **对齐基准**：`tests/gravity_vectors.rs` 用源仓测试向量核对 `decide_level`（1e-9）。

use serde::Serialize;

/// 问题卡 / 碎片的档位。
///
/// 三个取值都由 [`decide_level`] 产出（没有"永远到不了"的档位）——
/// 源里那个 `compressed`（压缩库）在本项目没有对应物，故不复制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentLevel {
    /// 短期：问得少、也不特别重要——冷一下就能回来。
    Short,
    /// 中期：问过几次，或有点分量——冷却久一些。
    Mid,
    /// 长期：被反复问过，或非常重要——几乎不再自动冒头。
    Long,
}

impl FragmentLevel {
    /// 全部取值（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [FragmentLevel; 3] =
        [FragmentLevel::Short, FragmentLevel::Mid, FragmentLevel::Long];
}

/// 档位参数。阈值默认值与源一致（向量对齐靠它）。
///
/// `*_retention_days`：各档位的冷却天数；短期 2 天、中期 21 天、长期不给（不自动回池子）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateMachineParams {
    pub mid_term_threshold: usize,
    pub long_term_threshold: usize,
    pub importance_threshold: f64,
    pub short_term_retention_days: usize,
    pub mid_term_retention_days: usize,
}

impl Default for StateMachineParams {
    fn default() -> Self {
        Self {
            mid_term_threshold: 2,
            long_term_threshold: 7,
            importance_threshold: 0.8,
            short_term_retention_days: 2,
            mid_term_retention_days: 21,
        }
    }
}

/// 重要度过阈值 → 长期；提及 ≥ 长期阈值 → 长期；≥ 中期阈值 → 中期；否则短期。
pub fn decide_level(mentions: usize, importance: f64, p: &StateMachineParams) -> FragmentLevel {
    if importance >= p.importance_threshold || mentions >= p.long_term_threshold {
        FragmentLevel::Long
    } else if mentions >= p.mid_term_threshold {
        FragmentLevel::Mid
    } else {
        FragmentLevel::Short
    }
}

/// 冷却天数；长期为 `None`（不自动回候选池——想再问是作者自己的动作）。
pub fn retention_days(level: FragmentLevel, p: &StateMachineParams) -> Option<usize> {
    match level {
        FragmentLevel::Mid => Some(p.mid_term_retention_days),
        FragmentLevel::Short => Some(p.short_term_retention_days),
        FragmentLevel::Long => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_by_mentions_and_importance() {
        let p = StateMachineParams::default();
        assert_eq!(decide_level(0, 0.9, &p), FragmentLevel::Long);
        assert_eq!(decide_level(8, 0.0, &p), FragmentLevel::Long);
        assert_eq!(decide_level(3, 0.0, &p), FragmentLevel::Mid);
        assert_eq!(decide_level(0, 0.0, &p), FragmentLevel::Short);
    }

    #[test]
    fn retention_only_for_short_and_mid() {
        let p = StateMachineParams::default();
        assert_eq!(retention_days(FragmentLevel::Short, &p), Some(2));
        assert_eq!(retention_days(FragmentLevel::Mid, &p), Some(21));
        assert_eq!(retention_days(FragmentLevel::Long, &p), None);
    }
}
