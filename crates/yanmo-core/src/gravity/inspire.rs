//! 灵感引力（纯逻辑）：**一池碎片里，此刻最该被看见的是哪几条**。
//!
//! ```text
//! 引力 = 重要度 × 热度 × 关联度 × 新颖度
//! ```
//!
//! 四个乘数各自都能单独说清（界面因此回答得了"为什么先看它"）：
//!
//! | 乘数 | 从哪来 | 谁在改变它 |
//! |---|---|---|
//! | 重要度 | 碎片自己的 `importance`（作者标的，默认 0.5） | 作者 |
//! | 热度 | 记下的那一刻是 1，之后按 [`super::forgetting`] 的曲线衰减 | 时间 |
//! | 关联度 | 碎片锚点与**当前焦点**（这一章 / 这一卷）命中几条 | 作者写到哪儿 |
//! | 新颖度 | 被「采纳」过几次 + 距上次采纳多久（机制取自 [`super::novelty::novelty`]） | 用掉的那笔账 |
//!
//! 三条分寸：
//!
//! 1. **新鲜碎片优先**是硬口径：`used_count = 0` 的碎片新颖度恒为 1.0（见
//!    [`super::novelty::novelty`]），而热度正从 1.0 起落——所以"刚记下、还没用过"
//!    的那一条几乎总在最前，这正是灵感速记存在的意义；
//! 2. **没有焦点时不惩罚任何人**：作者只是翻看（没落在某一章）时，关联度一律给 1.0，
//!    排序退化成「重要度 × 热度 × 新颖度」——绝不因为"不知道他在哪"就乱压；
//! 3. **不猜、不生成**：这一层只排序，一个字都不产（要说的话由界面按码渲染）。
//!
//! 与叩问那条线的关系：两处的**机制同源**（衰减曲线 / 新颖度都是 [`super`] 里那一套），
//! 只是词的语义不同——那边"被问出"消耗新颖度，这边"被采纳"消耗新颖度。
//! 候选从哪来（读库、按作品与焦点筛）在 [`crate::store`]。

use serde::Serialize;

use super::forgetting::{decayed_strength, ForgettingParams};
use super::novelty::{days_between, novelty};

/// 灵感引力参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SparkParams {
    /// 衰减曲线——热度与新颖度**共用同一条**（与叩问那边也是同一条，机制不复制第二份）
    pub forgetting: ForgettingParams,
    /// 当前焦点里**一条锚点都没命中**时关联度给多少（不是 0：没命中 ≠ 没用）
    pub unrelated_floor: f64,
    /// 每命中一条锚点关联度涨多少（1.0 封顶）
    pub anchor_step: f64,
}

impl Default for SparkParams {
    fn default() -> Self {
        Self {
            forgetting: ForgettingParams::default(),
            unrelated_floor: 0.25,
            anchor_step: 0.5,
        }
    }
}

/// 一条候选碎片的事实——**纯函数只认它，不认识 SQL**（好测，命令行也能喂）。
#[derive(Debug, Clone, PartialEq)]
pub struct SparkCandidate {
    pub fragment_id: i64,
    /// 作者标的这条例有多重要（0~1）
    pub importance: f64,
    /// 记下的时刻——**热度的起点**
    pub created_at: i64,
    /// 被「采纳」过几次（库里的 `used_count`，在碎片这条线上语义是"用掉过几回"）
    pub used_count: i64,
    /// 上次被采纳的时刻；从没采纳过 = `None`
    pub last_used_at: Option<i64>,
    /// 关联锚点（`chapter:12` 这种）
    pub anchors: Vec<String>,
}

/// 引力的拆解——既给排序用，也给"为什么先看它"用。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SparkGravity {
    pub total: f64,
    pub importance: f64,
    pub heat: f64,
    pub relevance: f64,
    pub novelty: f64,
    /// 关联锚点命中了几条（拆解的一部分：界面能说清"为什么它在前面"）
    pub anchor_hits: usize,
}

/// **热度**：记下的那一刻为 1，之后按同一条衰减曲线往下走（**永不为零**，地板见曲线参数）。
///
/// 将来"被引用得多不多"要接进来时，**从同一个乘数注入**（取它与被引用活跃度的较大者），
/// 公式的形状与其余三项都不用动。
pub fn heat(created_at: i64, now_ms: i64, p: &ForgettingParams) -> f64 {
    decayed_strength(1.0, days_between(created_at, now_ms), p)
}

/// **关联度**：碎片锚点与当前焦点命中几条（返回关联度与命中数）。
///
/// 焦点是空的（作者在翻看，没落在某一章）= **一律 1.0**——不因为"不知道他在哪"乱压人。
pub fn relevance(anchors: &[String], focus: &[String], p: &SparkParams) -> (f64, usize) {
    if focus.is_empty() {
        return (1.0, 0);
    }
    let hits = anchors.iter().filter(|a| focus.contains(a)).count();
    if hits == 0 {
        (p.unrelated_floor, 0)
    } else {
        ((p.anchor_step * hits as f64).min(1.0), hits)
    }
}

/// 算一条碎片的灵感引力（拆解版）。
pub fn spark_gravity(
    c: &SparkCandidate,
    focus: &[String],
    now_ms: i64,
    p: &SparkParams,
) -> SparkGravity {
    let importance = c.importance.clamp(0.0, 1.0);
    let heat = heat(c.created_at, now_ms, &p.forgetting);
    let (relevance, anchor_hits) = relevance(&c.anchors, focus, p);
    let novelty = novelty(c.used_count, c.last_used_at, now_ms, &p.forgetting);
    SparkGravity {
        total: importance * heat * relevance * novelty,
        importance,
        heat,
        relevance,
        novelty,
        anchor_hits,
    }
}

/// 排序：引力降序；同分按碎片 id 升序——**同样的输入永远同样的顺序**（好复核、好写测试）。
pub fn rank(mut scored: Vec<(i64, SparkGravity)>) -> Vec<(i64, SparkGravity)> {
    scored.sort_by(|(a_id, a), (b_id, b)| b.total.total_cmp(&a.total).then(a_id.cmp(b_id)));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    fn candidate(id: i64, created_at: i64) -> SparkCandidate {
        SparkCandidate {
            fragment_id: id,
            importance: 0.5,
            created_at,
            used_count: 0,
            last_used_at: None,
            anchors: Vec::new(),
        }
    }

    fn ids(scored: &[(i64, SparkGravity)]) -> Vec<i64> {
        scored.iter().map(|(id, _)| *id).collect()
    }

    /// 产品口径的第一条：**新鲜碎片优先**（刚记下的压过放旧的）。
    #[test]
    fn fresh_beats_stale_when_everything_else_is_equal() {
        let p = SparkParams::default();
        let now = 100 * DAY;
        let scored = rank(vec![
            (1, spark_gravity(&candidate(1, now - 60 * DAY), &[], now, &p)),
            (2, spark_gravity(&candidate(2, now), &[], now, &p)),
            (3, spark_gravity(&candidate(3, now - 3 * DAY), &[], now, &p)),
        ]);
        assert_eq!(ids(&scored), vec![2, 3, 1], "越新越前");
    }

    /// 「用掉过」的账：采纳过一次的碎片新颖度掉一截，排到没采纳过的后面。
    #[test]
    fn used_sparks_fall_behind_unused_ones() {
        let p = SparkParams::default();
        let now = 10 * DAY;
        let fresh = candidate(1, now);
        let mut used = candidate(2, now);
        used.used_count = 1;
        used.last_used_at = Some(now);
        let scored = rank(vec![
            (1, spark_gravity(&fresh, &[], now, &p)),
            (2, spark_gravity(&used, &[], now, &p)),
        ]);
        assert_eq!(ids(&scored), vec![1, 2], "没用过的在前");
        assert!(scored[1].1.novelty < 1.0 && scored[0].1.novelty == 1.0);
    }

    /// 焦点里命中的锚点越多，关联度越高；一条都没命中给地板（**不是 0**）。
    #[test]
    fn relevance_follows_the_current_focus() {
        let p = SparkParams::default();
        let focus = vec!["chapter:12".to_string(), "volume:3".to_string()];
        let one = SparkCandidate {
            anchors: vec!["chapter:12".to_string()],
            ..candidate(1, 0)
        };
        let two = SparkCandidate {
            anchors: vec!["chapter:12".to_string(), "volume:3".to_string()],
            ..candidate(2, 0)
        };
        let none = SparkCandidate {
            anchors: vec!["chapter:99".to_string()],
            ..candidate(3, 0)
        };
        assert_eq!(spark_gravity(&one, &focus, 0, &p).relevance, 0.5);
        assert_eq!(spark_gravity(&two, &focus, 0, &p).relevance, 1.0);
        let g = spark_gravity(&none, &focus, 0, &p);
        assert_eq!(g.relevance, p.unrelated_floor);
        assert_eq!(g.anchor_hits, 0, "没命中就不报命中");
    }

    /// 没有焦点（作者只是翻看）时谁都不被压：关联度一律 1.0。
    #[test]
    fn no_focus_never_punishes_anyone() {
        let p = SparkParams::default();
        let orphan = SparkCandidate {
            anchors: vec!["chapter:99".to_string()],
            ..candidate(1, 0)
        };
        let g = spark_gravity(&orphan, &[], 0, &p);
        assert_eq!(g.relevance, 1.0);
        assert_eq!(g.anchor_hits, 0);
    }

    /// 重要度是作者唯一的手动旋钮，但**它有个预算**：翻倍（0.5 → 1.0）压得住几天衰老，
    /// 压不住"放了二十天"。这条曲线是产品口径的一部分，钉住它免得将来被悄悄改掉。
    #[test]
    fn importance_is_the_authors_knob_but_it_has_a_budget() {
        let p = SparkParams::default();
        let now = 5 * DAY;
        let plain = candidate(1, now);
        let mut starred = candidate(2, now - 5 * DAY);
        starred.importance = 1.0;
        let mut very_old = candidate(3, now - 20 * DAY);
        very_old.importance = 1.0;
        let scored = rank(vec![
            (1, spark_gravity(&plain, &[], now, &p)),
            (2, spark_gravity(&starred, &[], now, &p)),
            (3, spark_gravity(&very_old, &[], now, &p)),
        ]);
        assert_eq!(
            ids(&scored),
            vec![2, 1, 3],
            "标了 1.0 的压得过 5 天的衰老（e^-0.5），压不过 20 天的（e^-2）"
        );
    }

    /// 同分按 id 升序：同样的输入**永远同样的顺序**（可复核、可回归）。
    #[test]
    fn ties_break_by_id_so_the_order_is_stable() {
        let p = SparkParams::default();
        let now = DAY;
        let scored = rank(vec![
            (7, spark_gravity(&candidate(7, now), &[], now, &p)),
            (3, spark_gravity(&candidate(3, now), &[], now, &p)),
            (5, spark_gravity(&candidate(5, now), &[], now, &p)),
        ]);
        assert_eq!(ids(&scored), vec![3, 5, 7]);
    }
}
