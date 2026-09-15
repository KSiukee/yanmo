//! 引力（纯逻辑）：**该不该现在问、先问哪个**。
//!
//! ```text
//! 引力 = 时机紧迫度 × 要素重要度 × 新颖度 × 派生折扣 × 模板权重
//! ```
//!
//! 五个乘数的分工（每一项都能单独说清，将来界面能回答"为什么问这个"）：
//!
//! | 乘数 | 从哪来 | 谁在长大 |
//! |---|---|---|
//! | 时机紧迫度 | 模板的基础紧迫度（`base_urgency`） | L1 只到这一层；接伏笔超期 / 五线推进度 / 切入时机判据是后续的智能版，**从同一个入口注入**，公式不改 |
//! | 要素重要度 | 卡上的 `importance`（建卡时定） | 建卡的人 / 模块 |
//! | 新颖度 | 被问过几次 + 距上次问出多久（机制取自 [`super::forgetting`]） | 「不是永远那几个问题」就靠它 |
//! | 派生折扣 | 是不是问题勾出来的灵感再派生的问题（×0.5） | 防自激：**只影响排序，不禁用** |
//! | 模板权重 | 偏好学习闭环（作答 / 说好 / 舍弃 / 静音） | 越用越懂你 |
//!
//! 「进冷却」不是删除、也不是不给看：冷却中的卡引力压到 [`AttractorParams::cooled_weight`]
//! （默认 0.05），在候选池里排到后面——作者主动翻（pull）照样看得见、答得了。

use serde::Serialize;

use super::novelty::{days_between, is_cooled, novelty};
use super::forgetting::ForgettingParams;
use super::state_machine::{decide_level, FragmentLevel, StateMachineParams};

/// 选题参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttractorParams {
    /// 衰减与"沉下去"阈值
    pub forgetting: ForgettingParams,
    /// 档位与冷却期
    pub levels: StateMachineParams,
    /// 派生问题的折扣（只影响排序，不禁用）
    pub derived_discount: f64,
    /// 自动派生链的深度上限（作者手动不受此限）
    pub max_derived_depth: usize,
    /// 延后几次之后开始降权（防死循环）
    pub max_deferrals: usize,
    /// 延后降权的下限（**不清零**：作者主动翻还看得见）
    pub defer_penalty_floor: f64,
    /// 冷却中的卡压到这个权重（不是 0：主动翻仍看得见）
    pub cooled_weight: f64,
}

impl Default for AttractorParams {
    fn default() -> Self {
        Self {
            forgetting: ForgettingParams::default(),
            levels: StateMachineParams::default(),
            derived_discount: 0.5,
            max_derived_depth: 2,
            max_deferrals: 3,
            defer_penalty_floor: 0.1,
            cooled_weight: 0.05,
        }
    }
}

/// 一张候选卡的事实——**纯函数只认它，不认识 SQL**（好测、也能被命令行喂）。
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub card_id: i64,
    /// 模板基础紧迫度（要素有多"到时候了"）
    pub base_urgency: f64,
    pub importance: f64,
    pub used_count: i64,
    /// 上次被问出的时刻；从没问过 = `None`
    pub last_asked_at: Option<i64>,
    /// 是不是**系统自动派生**出来的问题（作者手动基于灵感再问的那种不算）
    pub auto_derived: bool,
    /// 这张卡被延后过几次（防死循环的账：延后多了就往下压）
    pub defer_count: usize,
    /// 同类模板的学习权重（1.0 = 还没学过）
    pub template_weight: f64,
    /// 这一类模板被永久静音了吗
    pub template_muted: bool,
}

/// 引力的拆解——既给排序用，也给"为什么问这个"用。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gravity {
    pub total: f64,
    pub timeliness: f64,
    pub importance: f64,
    pub novelty: f64,
    pub derived_discount: f64,
    pub template_weight: f64,
    /// 延后降权（延后次数多到一定程度就往下压，**不清零**）
    pub defer_penalty: f64,
    /// 这张卡现在处在哪个冷却档位
    pub level: FragmentLevel,
    /// 是不是在冷却里（排到后面，但仍看得见）
    pub cooled: bool,
}

/// 算一张卡的引力（拆解版）。
pub fn gravity(c: &Candidate, now_ms: i64, p: &AttractorParams) -> Gravity {
    let level = decide_level(c.used_count.max(0) as usize, c.importance, &p.levels);
    let days = c.last_asked_at.map(|at| days_between(at, now_ms)).unwrap_or(f64::INFINITY);
    let novelty = novelty(c.used_count, c.last_asked_at, now_ms, &p.forgetting);
    let cooled = is_cooled(novelty, level, days, p);
    let timeliness = c.base_urgency.clamp(0.0, 1.0);
    let importance = c.importance.clamp(0.0, 1.0);
    // 折扣只打给"系统自动派生"的问题；作者自己顺着灵感问的问题不打折（别约束作者）
    let derived_discount = if c.auto_derived { p.derived_discount } else { 1.0 };
    // 静音的模板一头压到 0：候选池那边还会直接滤掉它（双保险，别指望调用方记得）
    let template_weight = if c.template_muted { 0.0 } else { c.template_weight.max(0.0) };
    let defer_penalty = defer_penalty(c.defer_count, p);
    let mut total =
        timeliness * importance * novelty * derived_discount * template_weight * defer_penalty;
    if cooled {
        total *= p.cooled_weight;
    }
    Gravity {
        total,
        timeliness,
        importance,
        novelty,
        derived_discount,
        template_weight,
        defer_penalty,
        level,
        cooled,
    }
}

/// **延后降权**（防死循环）：同一问题延后到第 `max_deferrals` 次起，引力每多一次对折一次，
/// 一直到下限 [`AttractorParams::defer_penalty_floor`]。
///
/// 「不硬插队」的另一半：延后过的卡回到池子时排得**后面一点**，而不是被禁掉——
/// 作者主动翻（pull）照样看得见、答得了。
pub fn defer_penalty(defer_count: usize, p: &AttractorParams) -> f64 {
    if defer_count < p.max_deferrals {
        return 1.0;
    }
    let extra = (defer_count - p.max_deferrals) as i32;
    (0.5_f64.powi(extra + 1)).max(p.defer_penalty_floor)
}

/// 排序：引力降序；同分按卡 id 升序——**同样的输入永远同样的顺序**（好复核、好写测试）。
pub fn rank(mut scored: Vec<(i64, Gravity)>) -> Vec<(i64, Gravity)> {
    scored.sort_by(|(a_id, a), (b_id, b)| b.total.total_cmp(&a.total).then(a_id.cmp(b_id)));
    scored
}

/// 防自激的第一道闸：**系统自动派生**的问题，链深到上限就不再往下派生。
///
/// 注意分寸：它只拦"系统自动派生"，**不拦作者**——作者基于一条灵感主动再问，不受此限。
pub fn allowed_auto_derivation(depth: usize, max_depth: usize) -> bool {
    depth < max_depth
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(used: i64, asked_at: Option<i64>) -> Candidate {
        Candidate {
            card_id: 1,
            base_urgency: 1.0,
            importance: 1.0,
            used_count: used,
            last_asked_at: asked_at,
            auto_derived: false,
            defer_count: 0,
            template_weight: 1.0,
            template_muted: false,
        }
    }

    #[test]
    fn gravity_multiplies_and_cools() {
        let p = AttractorParams::default();
        let now = 1_000 * 86_400_000;
        let g = gravity(&candidate(0, None), now, &p);
        assert!((g.total - 1.0).abs() < 1e-12, "全新、重要、紧迫、权重的卡引力是 1：{g:?}");

        let cooled = gravity(&candidate(7, Some(now)), now, &p);
        assert!(cooled.cooled && cooled.total < 0.05, "问过七次的卡该沉下去：{cooled:?}");

        let derived = gravity(
            &Candidate { auto_derived: true, ..candidate(0, None) },
            now,
            &p,
        );
        assert!((derived.total - 0.5).abs() < 1e-12, "派生问题打对折：{derived:?}");

        let muted = gravity(
            &Candidate { template_muted: true, ..candidate(0, None) },
            now,
            &p,
        );
        assert_eq!(muted.total, 0.0, "静音的那一类引力为零");
    }

    /// 防死循环：延后到第三次起对折，每多一次再对折，**有下限、不清零**。
    #[test]
    fn repeated_deferrals_push_the_card_back_without_killing_it() {
        let p = AttractorParams::default();
        assert_eq!(defer_penalty(0, &p), 1.0);
        assert_eq!(defer_penalty(2, &p), 1.0, "没到次数不罚");
        assert_eq!(defer_penalty(3, &p), 0.5);
        assert_eq!(defer_penalty(4, &p), 0.25);
        assert_eq!(defer_penalty(9, &p), p.defer_penalty_floor, "一路对折也不清零");

        let now = 1_000 * 86_400_000;
        let g = gravity(&Candidate { defer_count: 3, ..candidate(0, None) }, now, &p);
        assert_eq!(g.defer_penalty, 0.5);
        assert!((g.total - 0.5).abs() < 1e-12, "降权落在总引力上：{g:?}");
    }

    #[test]
    fn rank_is_stable_on_ties() {
        let g = Gravity {
            total: 0.5,
            timeliness: 1.0,
            importance: 1.0,
            novelty: 0.5,
            derived_discount: 1.0,
            template_weight: 1.0,
            defer_penalty: 1.0,
            level: FragmentLevel::Short,
            cooled: false,
        };
        let order = rank(vec![(9, g.clone()), (3, g.clone()), (5, g)]);
        assert_eq!(order.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![3, 5, 9]);
    }
}
