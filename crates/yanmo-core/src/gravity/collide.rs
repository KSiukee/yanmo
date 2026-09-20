//! 碰撞（纯逻辑）：**把两条碎片摆到一起，问一句"它们之间是什么关系"**。
//!
//! 机制只管两件事：
//!
//! 1. **挑对**：哪两条值得放一起——确定性规则，同输入永远同输出（可复核、可回归）；
//! 2. **说清关系**：`together`（有共用锚点：手边的线，拧成一股）或 `apart`
//!    （连锚点都不搭：离得远，撞一下也许出新东西）。
//!
//! **一个字都不生成**：撞出来的那句话由作者写（界面按字典渲染一句引子扶一把，见
//! [`crate::store`] 那一层的读写）。这一层不碰 LLM、不碰网络、不碰数据库——
//! 它只认 [`CollisionFact`] 那点事实。
//!
//! # 四处分寸
//!
//! - **一条碎片一次只上一场**（贪心）：一屏摆三对，其中重复出现同一条会让人误以为
//!   "就这一条有戏"；
//! - **撞过的组合不再撞**：`seen` 由调用方给（它认的是库里那些**双源溯源**的卡，
//!   以及作者按过"跳过"的那些对），这一层只管跳过它们；
//! - **撞出来的念头不再撞回自己的源头**：那是把同一件事再想一遍，不是"撞一下"
//!   （源头认的是锚点里的 `spark:<id>`——碰撞卡的溯源就刻在那儿）；
//! - **一条强一条弱不算好撞**：分数取两条里**较弱**的那条引力——强的那条自己就排在前面，
//!   不需要靠一条弱的来抬；
//! - **同分按 id 升序**：同样的输入永远同样的顺序（好复核、好写测试）。

use serde::Serialize;

/// 两条碎片的关系（稳定码进 JSON 与留痕）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionRelation {
    /// 同处：有共用的关联锚点（同一章 / 同一处）。
    Together,
    /// 相隔：没有共用锚点（含两边都没锚点的情形）——离得远才可能撞出新东西。
    Apart,
}

impl CollisionRelation {
    /// 全部取值（界面对表用）。
    pub const ALL: [CollisionRelation; 2] = [CollisionRelation::Together, CollisionRelation::Apart];

    /// 稳定码。
    pub const fn as_str(self) -> &'static str {
        match self {
            CollisionRelation::Together => "together",
            CollisionRelation::Apart => "apart",
        }
    }
}

/// 一条碎片参与碰撞时要给的事实（**纯函数只认它，不认识 SQL**）。
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionFact {
    pub fragment_id: i64,
    /// 它的引力（越靠前的越值得撞）
    pub total: f64,
    /// 它的关联锚点
    pub anchors: Vec<String>,
}

/// 碰撞参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollideParams {
    /// 同处的权重（手边的线拧成一股，比硬凑更常有用，所以默认给得高）
    pub together_weight: f64,
    /// 相隔的权重
    pub apart_weight: f64,
}

impl Default for CollideParams {
    fn default() -> Self {
        Self { together_weight: 1.0, apart_weight: 0.6 }
    }
}

/// 一对的碰撞建议。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollisionPair {
    /// 小的 id 在前——**同一条组合永远同一个写法**（去重、"撞过没有"、留痕都认它）
    pub left: i64,
    pub right: i64,
    pub relation: CollisionRelation,
    /// 共用的那几条锚点（相隔时是空的）
    pub shared_anchors: Vec<String>,
    /// 碰撞分 = 两条里较弱的那条引力 × 关系权重
    pub score: f64,
}

/// 两条 id 的**规范写法**（小的在前）。
pub fn order(a: i64, b: i64) -> (i64, i64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// 两条碎片的关系与**共用的那几条锚点**（相隔时是空数组）。
pub fn relation_of(left: &[String], right: &[String]) -> (CollisionRelation, Vec<String>) {
    let shared: Vec<String> = left
        .iter()
        .filter(|anchor| right.contains(anchor))
        .cloned()
        .collect();
    let relation = if shared.is_empty() {
        CollisionRelation::Apart
    } else {
        CollisionRelation::Together
    };
    (relation, shared)
}

/// 一条是不是另一条的**碰撞源头**（对方的锚点里带着 `spark:<它的 id>`）。
///
/// 认锚点而不是另查一次库：碰撞卡的溯源本来就刻在 `linked` 里（见 [`crate::store`]），
/// 于是这条规则也留在纯逻辑这一层——不额外要一个事实。
fn is_source_of(source: &CollisionFact, other: &CollisionFact) -> bool {
    let tag = format!("spark:{}", source.fragment_id);
    other.anchors.iter().any(|anchor| *anchor == tag)
}

/// 两条里**较弱**的那条引力（格负为 0：引力不该是负数，真出现了也不让它把分数翻正）。
fn weaker(a: &CollisionFact, b: &CollisionFact) -> f64 {
    a.total.min(b.total).max(0.0)
}

fn weight(relation: CollisionRelation, p: &CollideParams) -> f64 {
    match relation {
        CollisionRelation::Together => p.together_weight,
        CollisionRelation::Apart => p.apart_weight,
    }
}

/// 挑对：**贪心**——所有组合按分数降序（同分按 id 升序）过一遍，
/// 取到一条就把两端的碎片都从池子里划掉，直到取满 `limit` 对。
///
/// `seen` 是**撞过的那些组合**（必须是 [`order`] 规范化过的写法）：跳过它们。
pub fn pick_pairs(
    facts: &[CollisionFact],
    seen: &[(i64, i64)],
    limit: usize,
    p: &CollideParams,
) -> Vec<CollisionPair> {
    let mut scored: Vec<CollisionPair> = Vec::new();
    for (index, a) in facts.iter().enumerate() {
        for b in facts.iter().skip(index + 1) {
            let (left, right) = order(a.fragment_id, b.fragment_id);
            if seen.iter().any(|(x, y)| *x == left && *y == right) {
                continue;
            }
            if is_source_of(a, b) || is_source_of(b, a) {
                continue;
            }
            let (relation, shared_anchors) = relation_of(&a.anchors, &b.anchors);
            scored.push(CollisionPair {
                left,
                right,
                relation,
                shared_anchors,
                score: weaker(a, b) * weight(relation, p),
            });
        }
    }
    scored.sort_by(|x, y| {
        y.score
            .total_cmp(&x.score)
            .then(x.left.cmp(&y.left))
            .then(x.right.cmp(&y.right))
    });
    let mut used: Vec<i64> = Vec::new();
    let mut out = Vec::new();
    for pair in scored {
        if out.len() >= limit {
            break;
        }
        if used.contains(&pair.left) || used.contains(&pair.right) {
            continue;
        }
        used.push(pair.left);
        used.push(pair.right);
        out.push(pair);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(id: i64, total: f64, anchors: &[&str]) -> CollisionFact {
        CollisionFact {
            fragment_id: id,
            total,
            anchors: anchors.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn pairs(facts: &[CollisionFact], seen: &[(i64, i64)], limit: usize) -> Vec<(i64, i64)> {
        pick_pairs(facts, seen, limit, &CollideParams::default())
            .into_iter()
            .map(|p| (p.left, p.right))
            .collect()
    }

    #[test]
    fn shared_anchors_make_it_together_and_name_them() {
        let (relation, shared) = relation_of(
            &["chapter:1".to_string(), "volume:1".to_string()],
            &["volume:1".to_string(), "chapter:9".to_string()],
        );
        assert_eq!(relation, CollisionRelation::Together);
        assert_eq!(shared, vec!["volume:1".to_string()], "只报真共用的那几条");

        let (relation, shared) = relation_of(&["chapter:1".to_string()], &["chapter:9".to_string()]);
        assert_eq!(relation, CollisionRelation::Apart);
        assert!(shared.is_empty());

        // 两边都没锚点也算"相隔"（不慌、不猜，如实说"搭不上"）
        let (relation, shared) = relation_of(&[], &[]);
        assert_eq!(relation, CollisionRelation::Apart);
        assert!(shared.is_empty());
    }

    #[test]
    fn together_outranks_apart_when_the_two_are_equally_strong() {
        let facts = vec![
            fact(1, 0.5, &["chapter:1"]),
            fact(2, 0.5, &["chapter:1"]),
            fact(3, 0.5, &["chapter:9"]),
            fact(4, 0.5, &["chapter:8"]),
        ];
        let got = pick_pairs(&facts, &[], 2, &CollideParams::default());
        assert_eq!((got[0].left, got[0].right), (1, 2), "同样强时先撞有共用的那对");
        assert_eq!(got[0].relation, CollisionRelation::Together);
        assert_eq!(got[1].relation, CollisionRelation::Apart, "剩下那两条锚点不搭");
        assert!(
            got[0].score > got[1].score,
            "同处那一对分数更高：{} / {}",
            got[0].score,
            got[1].score
        );
    }

    #[test]
    fn the_score_follows_the_weaker_one() {
        // 一条很强配一条很弱：分数按弱的那条算（强的那条自己就排在前面，不用靠弱的抬）
        let facts = vec![fact(1, 1.0, &["chapter:1"]), fact(2, 0.1, &["chapter:1"])];
        let got = pick_pairs(&facts, &[], 1, &CollideParams::default());
        assert!((got[0].score - 0.1).abs() < 1e-12, "分数该是 0.1：{}", got[0].score);
    }

    #[test]
    fn seen_pairs_are_skipped() {
        let facts = vec![
            fact(1, 0.9, &["chapter:1"]),
            fact(2, 0.9, &["chapter:1"]),
            fact(3, 0.5, &["chapter:9"]),
            fact(4, 0.5, &["chapter:9"]),
        ];
        assert_eq!(pairs(&facts, &[(1, 2)], 2), vec![(3, 4)], "撞过的那对不再撞");
    }

    /// 撞出来的念头身上刻着源头的 id：不再跟自己的源头撞回去。
    #[test]
    fn a_collision_card_never_collides_back_with_its_sources() {
        let facts = vec![
            fact(1, 0.9, &["chapter:1"]),
            fact(2, 0.9, &["chapter:1"]),
            // 3 是 1 与 2 撞出来的那张卡：锚点里带着两条溯源
            fact(3, 0.9, &["chapter:1", "spark:1", "spark:2"]),
        ];
        let got = pairs(&facts, &[], 3);
        assert_eq!(got, vec![(1, 2)], "1×3 与 2×3 都该跳过（那是它的源头）");
    }

    #[test]
    fn one_spark_only_takes_one_slot() {
        // 2 号跟 1、3 都撞得上，但一屏里它只上一场
        let facts = vec![
            fact(1, 0.9, &["chapter:1"]),
            fact(2, 0.9, &["chapter:1"]),
            fact(3, 0.9, &["chapter:1"]),
        ];
        let got = pairs(&facts, &[], 3);
        assert_eq!(got.len(), 1, "三条只够凑一对：{got:?}");
    }

    #[test]
    fn the_order_is_stable_for_the_same_input() {
        let facts = vec![
            fact(7, 0.5, &[]),
            fact(3, 0.5, &[]),
            fact(5, 0.5, &[]),
            fact(9, 0.5, &[]),
        ];
        // 分全一样：按规范写法（小 id 在前）的字典序排——同样的输入永远同样的顺序
        assert_eq!(pairs(&facts, &[], 2), vec![(3, 5), (7, 9)]);
        assert_eq!(pairs(&facts, &[], 2), pairs(&facts, &[], 2));
    }

    #[test]
    fn order_normalizes_both_ways() {
        assert_eq!(order(9, 2), (2, 9));
        assert_eq!(order(2, 9), (2, 9));
        assert_eq!(order(4, 4), (4, 4));
    }
}
