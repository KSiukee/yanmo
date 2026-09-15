//! 偏好学习闭环（纯逻辑）：**作者怎么处置，就怎么教同类模板**。
//!
//! 设计口径（四类信号，纯计数与权重，零 LLM）：
//!
//! | 作者行为 | 信号 | 对同类模板的影响 |
//! |---|---|---|
//! | 作答 | 中性 | **保持不变** |
//! | 说「这个问题好」 | 强正 | 加权 |
//! | 舍弃 | 负样本 | 降权 |
//! | 永久静音 | 强负 | 该类整体停用 |
//!
//! 两个分寸：
//!
//! 1. **信号由动作码自动折算**（[`signal_for_action`]）——处置与教学是同一件事，
//!    不靠界面记得再喊一声"顺便教一下"（那必然会被漏掉）。
//! 2. **停用是可撤销的**：静音把 `enabled` 置否、权重压到下限，但记录与计数都留着，
//!    将来解除静音（或作者改主意）时有据可依。
//!
//! 非模板产的问题（作者自己写的、模块提交的）没有同类模板可教——闭环里**直接跳过**，
//! 这不是失败，是"没有同类"。

use serde::Serialize;

/// 折算后的一条偏好信号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSignal {
    /// 说「这个问题好」——强正反馈
    Praised,
    /// 舍弃——负样本
    Discarded,
    /// 永久静音——这一类整体停用
    Muted,
}

/// 一个模板的学习状态：权重 + 是否停用 + 正负样本计数。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TemplateWeight {
    pub weight: f64,
    pub enabled: bool,
    pub positives: i64,
    pub negatives: i64,
}

impl Default for TemplateWeight {
    fn default() -> Self {
        Self { weight: 1.0, enabled: true, positives: 0, negatives: 0 }
    }
}

/// 学习率与边界。**有下限**：降权不等于禁用（静音才禁用）；**有上限**：一个人夸两次
/// 不该让某一类问题霸屏。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreferenceParams {
    pub praise_factor: f64,
    pub discard_factor: f64,
    pub min_weight: f64,
    pub max_weight: f64,
}

impl Default for PreferenceParams {
    fn default() -> Self {
        Self { praise_factor: 1.5, discard_factor: 0.6, min_weight: 0.05, max_weight: 4.0 }
    }
}

/// 把一条信号喂进去，得到新的学习状态（纯函数：同样的输入永远同样的结果）。
pub fn apply(
    current: &TemplateWeight,
    signal: FeedbackSignal,
    p: &PreferenceParams,
) -> TemplateWeight {
    match signal {
        FeedbackSignal::Praised => TemplateWeight {
            weight: (current.weight * p.praise_factor).min(p.max_weight),
            enabled: current.enabled,
            positives: current.positives + 1,
            negatives: current.negatives,
        },
        FeedbackSignal::Discarded => TemplateWeight {
            weight: (current.weight * p.discard_factor).max(p.min_weight),
            enabled: current.enabled,
            positives: current.positives,
            negatives: current.negatives + 1,
        },
        FeedbackSignal::Muted => TemplateWeight {
            weight: p.min_weight,
            enabled: false,
            positives: current.positives,
            negatives: current.negatives + 1,
        },
    }
}

/// **动作码 → 偏好信号**（`None` = 中性，不教）。
///
/// 这是状态机与学习闭环之间**唯一**的一张对照表：加一条迁移，就在这里表个态
/// （`tests/question_engine.rs` 会遍历迁移表核对每种动作都被想过一遍）。
pub fn signal_for_action(action: &str) -> Option<FeedbackSignal> {
    match action {
        // 作者答了：中性（"他愿意答"不等于"他喜欢这类问题"）
        "answer" => None,
        // 还没问就被划掉、或答不上来也不想要：负样本
        "discard" => Some(FeedbackSignal::Discarded),
        // 这一类以后都别问了：强负 + 停用
        "mute" => Some(FeedbackSignal::Muted),
        // 问出 / 延后 / 重出 / 捞回 / 解除静音：都不改变偏好
        "ask" | "defer" | "requeue" | "retrieve" | "unmute" => None,
        // 认不出来的动作码：不猜（宁可什么都不学，也别学错）
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn praise_lifts_and_discard_lowers_within_bounds() {
        let p = PreferenceParams::default();
        let start = TemplateWeight::default();

        let praised = apply(&start, FeedbackSignal::Praised, &p);
        assert!((praised.weight - 1.5).abs() < 1e-12 && praised.positives == 1);

        let discarded = apply(&start, FeedbackSignal::Discarded, &p);
        assert!((discarded.weight - 0.6).abs() < 1e-12 && discarded.negatives == 1);

        // 一路夸到上限、一路贬到下限：都不越界
        let mut high = start;
        for _ in 0..20 {
            high = apply(&high, FeedbackSignal::Praised, &p);
        }
        assert_eq!(high.weight, p.max_weight);
        let mut low = start;
        for _ in 0..20 {
            low = apply(&low, FeedbackSignal::Discarded, &p);
        }
        assert_eq!(low.weight, p.min_weight);
        assert!(low.enabled, "降权不等于禁用——禁用只有静音那一条路");
    }

    #[test]
    fn mute_disables_and_keeps_the_history() {
        let p = PreferenceParams::default();
        let praised = apply(&TemplateWeight::default(), FeedbackSignal::Praised, &p);
        let muted = apply(&praised, FeedbackSignal::Muted, &p);
        assert!(!muted.enabled);
        assert_eq!(muted.weight, p.min_weight);
        assert_eq!(muted.positives, 1, "夸过的记录留着（解除静音时有据可依）");
        assert_eq!(muted.negatives, 1);
    }
}
