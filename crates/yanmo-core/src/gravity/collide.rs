//! 碰撞（纯逻辑）：**把两头摆到一起，问一句"它们之间是什么关系"**。
//!
//! 两头可以是**一条碎片**（作者随手记的），也可以是**一处大纲元素**——章纲（某一章的
//! "一句话"）、伏笔、人物卡。四者在这个文件里一视同仁：都摊成 [`CollisionFact`]
//! （一个标识 + 一份分量 + 一组关联锚点），于是规则只有一套。
//! 标识本身（种类 + 编号 + 规范写法）在 [`crate::model::CollisionSide`]。
//!
//! 机制只管两件事：
//!
//! 1. **挑对**：哪两头值得放一起——确定性规则，同输入永远同输出（可复核、可回归）；
//! 2. **说清关系**：`together`（有共用锚点）或 `apart`（连锚点都不搭）。
//!
//! **一个字都不生成**：撞出来的那句话由作者写（界面按关系渲染一句引子扶一把，见
//! [`crate::store`] 那一层的读写）。这一层不碰 LLM、不碰网络、不碰数据库。
//!
//! # 五处分寸
//!
//! - **每一对里至少一头是碎片**：元素跟元素撞（章纲 × 伏笔）不摆出来——那一屏是给
//!   "你记下的东西"用的，元素×元素会把三对全占满（v1 的边界，写在这儿免得忘）；
//! - **一头只上一场**（贪心）：一屏摆三对，其中重复出现同一头会让人误以为"就这条有戏"；
//! - **撞过的组合不再撞**：`seen` 由调用方给（它认的是库里那些**双源溯源**的卡，
//!   以及作者按过"跳过"的那些对），这一层只管跳过它们；
//! - **撞出来的念头不再撞回自己的源头**：那是把同一件事再想一遍；
//! - **一头强一头弱不算好撞**：分数取两头里**较弱**的那个分量——强的那头自己就排在前面，
//!   不需要靠弱的来抬。

use serde::Serialize;

use crate::model::{canonical_order, CollisionKind, CollisionSide};

/// 两头的关系（稳定码进 JSON 与留痕）。
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

/// 一头参与碰撞时要给的事实（**纯函数只认它，不认识 SQL**）。
///
/// 分量（`total`）对碎片是它自己的引力，对大纲元素是**中性满分 1.0**——
/// 元素没有作者标的重要度，机制也不该替它编一个（见 [`crate::store`] 那一层的说明）。
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionFact {
    pub side: CollisionSide,
    pub total: f64,
    /// 它的关联锚点（碎片与三种元素都摊成同一套 `chapter:<id>` 写法）
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

/// 一头的碰撞建议。
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionPair {
    /// 规范写法（小的在前）
    pub left: CollisionSide,
    pub right: CollisionSide,
    pub relation: CollisionRelation,
    /// 共用的那几条锚点（相隔时是空的）
    pub shared_anchors: Vec<String>,
    /// 碰撞分 = 两头里较弱的那份分量 × 关系权重
    pub score: f64,
}

/// 两头的关系与**共用的那几条锚点**（相隔时是空数组）。
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

/// 一头是不是**另一头的碰撞源头**（对方的锚点里带着它的规范写法 `kind:id`）。
///
/// **不分种类**：碰撞卡的溯源把两头的标识都刻进 `linked`，所以碎片与元素都能当源头
/// （"你刚把这条念头跟这一章的章纲撞出来的那句"，不该再跟同一处章纲撞回去）。
/// 认锚点而不是另查一次库：溯源本来就刻在那儿（见 [`crate::store`]）。
fn is_source_of(source: &CollisionFact, other: &CollisionFact) -> bool {
    let tag = source.side.write();
    other.anchors.iter().any(|anchor| *anchor == tag)
}

/// 两头里**较弱**的那份分量（负的当 0：真出现了也不让它把分数翻正）。
fn weaker(a: &CollisionFact, b: &CollisionFact) -> f64 {
    a.total.min(b.total).max(0.0)
}

fn weight(relation: CollisionRelation, p: &CollideParams) -> f64 {
    match relation {
        CollisionRelation::Together => p.together_weight,
        CollisionRelation::Apart => p.apart_weight,
    }
}

/// 这一对能不能上场：**至少一头是碎片**（元素×元素不摆）。
fn has_a_spark(a: &CollisionFact, b: &CollisionFact) -> bool {
    a.side.kind == CollisionKind::Spark || b.side.kind == CollisionKind::Spark
}

/// 挑对：**贪心**——所有组合按分数降序（同分按规范写法）过一遍，
/// 取到一条就把两头都从池子里划掉，直到取满 `limit` 对。
///
/// `seen` 是**撞过的那些组合**（必须是 [`canonical_order`] 规范化过的写法）：跳过它们。
pub fn pick_pairs(
    facts: &[CollisionFact],
    seen: &[(CollisionSide, CollisionSide)],
    limit: usize,
    p: &CollideParams,
) -> Vec<CollisionPair> {
    let mut scored: Vec<CollisionPair> = Vec::new();
    for (index, a) in facts.iter().enumerate() {
        for b in facts.iter().skip(index + 1) {
            if !has_a_spark(a, b) {
                continue;
            }
            let (left, right) = canonical_order(a.side, b.side);
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
            .then(x.left.rank().cmp(&y.left.rank()))
            .then(x.right.rank().cmp(&y.right.rank()))
    });
    let mut used: Vec<CollisionSide> = Vec::new();
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

    fn fact(kind: CollisionKind, id: i64, total: f64, anchors: &[&str]) -> CollisionFact {
        CollisionFact {
            side: CollisionSide::new(kind, id),
            total,
            anchors: anchors.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn spark(id: i64, total: f64, anchors: &[&str]) -> CollisionFact {
        fact(CollisionKind::Spark, id, total, anchors)
    }

    fn side(kind: CollisionKind, id: i64) -> CollisionSide {
        CollisionSide::new(kind, id)
    }

    fn pairs(
        facts: &[CollisionFact],
        seen: &[(CollisionSide, CollisionSide)],
        limit: usize,
    ) -> Vec<String> {
        pick_pairs(facts, seen, limit, &CollideParams::default())
            .into_iter()
            .map(|p| format!("{}×{}", p.left.write(), p.right.write()))
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
            spark(1, 0.5, &["chapter:1"]),
            spark(2, 0.5, &["chapter:1"]),
            spark(3, 0.5, &["chapter:9"]),
            spark(4, 0.5, &["chapter:8"]),
        ];
        let got = pick_pairs(&facts, &[], 2, &CollideParams::default());
        assert_eq!((got[0].left.id, got[0].right.id), (1, 2), "同样强时先撞有共用的那对");
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
        let facts = vec![spark(1, 1.0, &["chapter:1"]), spark(2, 0.1, &["chapter:1"])];
        let got = pick_pairs(&facts, &[], 1, &CollideParams::default());
        assert!((got[0].score - 0.1).abs() < 1e-12, "分数该是 0.1：{}", got[0].score);
    }

    #[test]
    fn seen_pairs_are_skipped() {
        let facts = vec![
            spark(1, 0.9, &["chapter:1"]),
            spark(2, 0.9, &["chapter:1"]),
            spark(3, 0.5, &["chapter:9"]),
            spark(4, 0.5, &["chapter:9"]),
        ];
        assert_eq!(
            pairs(&facts, &[(side(CollisionKind::Spark, 1), side(CollisionKind::Spark, 2))], 2),
            vec!["spark:3×spark:4"],
            "撞过的那对不再撞"
        );
    }

    /// 大纲元素也上场，而且关系按**共用的章锚点**判定：
    /// 那条碎片只上一场，所以同处的那一头赢（比相隔的分数高）。
    #[test]
    fn an_outline_element_takes_the_other_side() {
        let facts = vec![
            spark(1, 0.5, &["chapter:2"]),
            fact(CollisionKind::Plan, 7, 1.0, &["chapter:2"]),
            fact(CollisionKind::Foreshadow, 7, 1.0, &["chapter:9"]),
        ];
        let got = pick_pairs(&facts, &[], 3, &CollideParams::default());
        assert_eq!(got.len(), 1, "那条碎片只上一场：{got:?}");
        assert_eq!(got[0].right.write(), "plan:7", "同处那一头赢过相隔的");
        assert_eq!(got[0].relation, CollisionRelation::Together, "章纲在同一章 = 同处");

        // 换一条挂在第九章的碎片：这回搭上的是伏笔（埋在同一章 = 同处）
        let facts = vec![
            spark(2, 0.5, &["chapter:9"]),
            fact(CollisionKind::Plan, 7, 1.0, &["chapter:2"]),
            fact(CollisionKind::Foreshadow, 7, 1.0, &["chapter:9"]),
        ];
        let got = pick_pairs(&facts, &[], 3, &CollideParams::default());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].right.write(), "foreshadow:7");
        assert_eq!(got[0].relation, CollisionRelation::Together);
    }

    /// 元素×元素不摆出来（那一屏是给"你记下的东西"用的）。
    #[test]
    fn two_elements_never_pair_with_each_other() {
        let facts = vec![
            fact(CollisionKind::Plan, 1, 1.0, &["chapter:1"]),
            fact(CollisionKind::Foreshadow, 5, 1.0, &["chapter:1"]),
            fact(CollisionKind::Entity, 9, 1.0, &["chapter:1"]),
        ];
        assert!(pairs(&facts, &[], 3).is_empty(), "三头都是元素，一对都不该有");
    }

    /// 撞出来的念头身上刻着源头的 id：不再跟自己的源头撞回去。
    #[test]
    fn a_collision_card_never_collides_back_with_its_sources() {
        let facts = vec![
            spark(1, 0.9, &["chapter:1"]),
            spark(2, 0.9, &["chapter:1"]),
            // 3 是 1 与 2 撞出来的那张卡：锚点里带着两条溯源
            spark(3, 0.9, &["chapter:1", "spark:1", "spark:2"]),
        ];
        assert_eq!(pairs(&facts, &[], 3), vec!["spark:1×spark:2"], "1×3 与 2×3 都该跳过");
    }

    /// 元素也能当源头：刻着 `plan:1` 的那张卡不再跟那处章纲撞回去。
    #[test]
    fn a_collision_card_never_collides_back_with_an_element_source() {
        // 3 是 1 与章纲 1 撞出来的：锚点里带着两条溯源（碎片与元素各一条）
        let card = spark(3, 0.9, &["chapter:1", "spark:1", "plan:1"]);
        let plan = fact(CollisionKind::Plan, 1, 1.0, &["chapter:1"]);
        assert!(
            pairs(&[card.clone(), plan.clone()], &[], 2).is_empty(),
            "它跟自己的元素源头不该再撞"
        );
        // 换一处没被撞过的章纲：照撞
        let other_plan = fact(CollisionKind::Plan, 9, 1.0, &["chapter:1"]);
        assert_eq!(pairs(&[card, other_plan], &[], 2), vec!["spark:3×plan:9"]);
    }

    #[test]
    fn one_side_only_takes_one_slot() {
        let facts = vec![
            spark(1, 0.9, &["chapter:1"]),
            spark(2, 0.9, &["chapter:1"]),
            spark(3, 0.9, &["chapter:1"]),
        ];
        let got = pairs(&facts, &[], 3);
        assert_eq!(got.len(), 1, "三头只够凑一对：{got:?}");
    }

    #[test]
    fn the_order_is_stable_for_the_same_input() {
        let facts = vec![
            spark(7, 0.5, &[]),
            spark(3, 0.5, &[]),
            spark(5, 0.5, &[]),
            spark(9, 0.5, &[]),
        ];
        // 分全一样：按规范写法（种类、再 id）排——同样的输入永远同样的顺序
        assert_eq!(pairs(&facts, &[], 2), vec!["spark:3×spark:5", "spark:7×spark:9"]);
        assert_eq!(pairs(&facts, &[], 2), pairs(&facts, &[], 2));
    }
}
