//! 灵感的**碰撞**：哪两头值得放一起、撞出来的那句怎么存、不想要的怎么跳过。
//!
//! 挑对的规则是纯逻辑（[`crate::gravity::collide`]）；这里只做四件事：把碎片与三种大纲元素
//! 喂成同一形状的事实、把"撞过的"和"跳过的"两组组合读出来、把作者写下的那句存成一张
//! **双源溯源**的灵感卡、把"跳过"记在书里的一把记号上。
//!
//! # 一条碰撞卡长什么样
//!
//! 它就是一张普通的灵感卡（`frag_kind = 'idea'`，**不新开表、不新加列**），只是身上多两样：
//!
//! - `derived_from` 指**碎片那一头**（元素不是碎片，塞不进那个外键——两头里至少一头是
//!   碎片，这条由两层一起保证：挑对时与保存时都判）；
//! - `linked` 里除了两头的锚点，还多两条**溯源锚点**（`spark:12` / `plan:3` 这种规范写法）——
//!   第二头是谁、以及"这一对撞过没有"，都靠它们。
//!
//! # 「跳过」为什么要落盘
//!
//! 碰撞是**每次打开都重算**的（池子一直在变），所以"这一对我看过了、不想撞"必须记下来，
//! 否则同一对会永远摆在最前面——被无视的提醒等于没有提醒（与大纲那条忽略标记同一个道理，
//! 那本账在 [`super::inspire_skips`]：`settings` 里一把不透明记号，按书分、整把读写）。

use std::collections::HashMap;

use rusqlite::params;
use serde::Serialize;
use serde_json::json;

use super::inspire::Spark;
use super::inspire_elements::Element;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::collide::{
    pick_pairs, relation_of, CollideParams, CollisionFact, CollisionRelation,
};
use crate::model::{
    canonical_order, CollisionKind, CollisionSide, FragmentKind, InputSource,
};
use crate::question::anchors_of;
use crate::time::now_millis;

/// 一屏摆几对（再多就不是"撞一下"，是让你干活了）。
pub const COLLISIONS_PER_BOARD: usize = 3;

/// 碎片那一头的池子上限——只让引力靠前的这些上场：
/// 再往后的碎片本来就排不进"此刻该看"，也省得把组合数算成平方级的浪费。
const COLLIDE_POOL: usize = 60;

/// 大纲元素那一头**不拉低分数**的中性分量（满分）。
///
/// 元素没有作者标的重要度，也没有"被用过几次"的账——机制**不该替它编一个**。
/// 1.0 的含义就是"这一头不拖后腿"：配对分数因此等于碎片那一头的引力，
/// 也就是碎片越靠前，它跟元素的配对也越靠前。
const ELEMENT_WEIGHT: f64 = 1.0;

/// 碰撞的一头给界面看的那一份。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollisionSideView {
    /// 规范标识（`kind:id`）：界面保存 / 跳过时**原样交回来**，不自己拼
    pub key: String,
    /// 这一头是什么（界面按它挑徽标与说法）
    pub kind: CollisionKind,
    /// 要嵌进引子的那段字（碎片 / 章纲 / 伏笔是正文，人物卡是名字）
    pub text: String,
    /// 人物卡的备注（其余为空串）——作者的补充说明，界面单独摆一行
    pub note: String,
    /// 碎片那一头带着碎片本身（界面要显示怎么记下的 / 多会儿记的）；
    /// 元素那一头是 `None`
    pub spark: Option<Spark>,
}

/// 一对碰撞建议：两头 + 关系 + 分数（界面拿关系码渲染一句引子）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Collision {
    pub left: CollisionSideView,
    pub right: CollisionSideView,
    pub relation: CollisionRelation,
    /// 共用的那几条锚点（相隔时是空的）
    pub shared_anchors: Vec<String>,
    pub score: f64,
}

/// 存下一条碰撞卡的回执。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollisionReceipt {
    /// 新写下的那张灵感卡（双源溯源已经在它身上）
    pub spark_id: i64,
    /// 撞完之后这一屏该是什么（撞过的那对自动不再出现）
    pub collisions: Vec<Collision>,
}

/// 一头在库里认不回来了（元素被删了 / 章纲被清空 / 伏笔已经收了）。
fn missing_side(side: CollisionSide) -> Error {
    Error::invalid_with(codes::COLLISION_SIDE_NOT_FOUND, [("side", side.write())])
}

fn view_of_spark(spark: Spark) -> CollisionSideView {
    CollisionSideView {
        key: CollisionSide::new(CollisionKind::Spark, spark.id).write(),
        kind: CollisionKind::Spark,
        text: spark.body.clone(),
        note: String::new(),
        spark: Some(spark),
    }
}

fn view_of_element(element: Element) -> CollisionSideView {
    CollisionSideView {
        key: element.side.write(),
        kind: element.side.kind,
        text: element.text,
        note: element.note,
        spark: None,
    }
}

impl Store {
    /// **一屏碰撞建议**：碎片与大纲元素一起上场，按规则挑几对。
    pub fn spark_collisions(
        &self,
        work_id: i64,
        node_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Collision>> {
        let sparks: Vec<Spark> = self
            .scored_sparks(work_id, node_id)?
            .into_iter()
            .take(COLLIDE_POOL)
            .collect();
        let mut facts: Vec<CollisionFact> = Vec::new();
        let mut views: HashMap<CollisionSide, CollisionSideView> = HashMap::new();
        for spark in sparks {
            let side = CollisionSide::new(CollisionKind::Spark, spark.id);
            facts.push(CollisionFact {
                side,
                total: spark.gravity.total,
                anchors: spark.anchors.clone(),
            });
            views.insert(side, view_of_spark(spark));
        }
        for element in self.outline_elements(work_id, node_id)? {
            let side = element.side;
            facts.push(CollisionFact {
                side,
                total: ELEMENT_WEIGHT,
                anchors: element.anchors.clone(),
            });
            views.insert(side, view_of_element(element));
        }
        // 撞过的（已经成过对的）与作者按过"跳过"的，都不再上场
        let mut seen = self.collided_pairs(work_id)?;
        for pair in self.skipped_collisions(work_id)? {
            if !seen.contains(&pair) {
                seen.push(pair);
            }
        }
        Ok(pick_pairs(&facts, &seen, limit, &CollideParams::default())
            .into_iter()
            .filter_map(|pair| {
                Some(Collision {
                    left: views.get(&pair.left)?.clone(),
                    right: views.get(&pair.right)?.clone(),
                    relation: pair.relation,
                    shared_anchors: pair.shared_anchors,
                    score: pair.score,
                })
            })
            .collect())
    }

    /// **存下撞出来的那一句**：写成一张双源溯源的灵感卡（写卡 + 留痕，同一个事务）。
    ///
    /// 两头都给**规范写法**（`spark:12` / `plan:3`）；关系（同处 / 相隔）**由核心自己算**，
    /// 不要调用方告诉它——那是这一层的事实，让界面报一遍只会多一个能说岔的地方。
    /// 它进留痕，于是"这条是怎么撞出来的"看得见。
    ///
    /// 两头里**至少一头得是碎片**：元素×元素不进这一屏（挑对那一层就不摆），
    /// 保存这条路也照样拒——两条路一个口径。
    pub fn save_collision(
        &mut self,
        left: &str,
        right: &str,
        body: &str,
        node_id: Option<i64>,
        trigger: &str,
    ) -> Result<CollisionReceipt> {
        let body = body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::IDEA_BODY_EMPTY));
        }
        let (left, right) = canonical_order(CollisionSide::parse(left)?, CollisionSide::parse(right)?);
        if left == right {
            return Err(Error::invalid_with(
                codes::COLLISION_SAME_SIDE,
                [("side", left.write())],
            ));
        }
        // 碎片那一头先认（书以它为准）：两头都不是碎片就当场拒
        let spark_side = match (left.kind, right.kind) {
            (CollisionKind::Spark, _) => left,
            (_, CollisionKind::Spark) => right,
            _ => {
                return Err(Error::invalid_with(
                    codes::COLLISION_NEEDS_SPARK,
                    [("left", left.write()), ("right", right.write())],
                ))
            }
        };
        let spark = self.spark_row(spark_side.id)?;
        let work_id = spark.work_id.unwrap_or(0);
        // 另一头：也是碎片就读它那一行；是元素就按**这一本书**认它（查不到就是不在）
        let other = if spark_side == left { right } else { left };
        let other_anchors = match other.kind {
            CollisionKind::Spark => {
                let row = self.spark_row(other.id)?;
                if row.work_id.unwrap_or(0) != work_id {
                    return Err(Error::invalid_with(
                        codes::COLLISION_FOREIGN,
                        [("left", left.write()), ("right", right.write())],
                    ));
                }
                anchors_of(&row.linked)
            }
            _ => {
                self.element_by_side(work_id, other)?
                    .ok_or_else(|| missing_side(other))?
                    .anchors
            }
        };
        let spark_anchors = anchors_of(&spark.linked);
        // 新卡的锚点：两头的锚点合集 + 两条**溯源锚点**（第二头是谁就靠它）
        let mut anchors = spark_anchors.clone();
        for anchor in other_anchors
            .iter()
            .chain([&spark_side.write(), &other.write()])
        {
            if !anchors.contains(anchor) {
                anchors.push(anchor.clone());
            }
        }
        let (relation, _) = relation_of(&spark_anchors, &other_anchors);
        let linked = serde_json::to_string(&anchors).unwrap_or_else(|_| "[]".to_string());
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO fragments(work_id, frag_kind, body, source, status, importance,
                                   used_count, last_asked_at, linked, derived_from, auto_derived,
                                   template_key, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, 'pending', 0.5, 0, NULL, ?5, ?6, 0, '', ?7, ?7)",
            params![
                work_id,
                FragmentKind::Idea.as_str(),
                body,
                InputSource::Typed.as_str(),
                linked,
                spark_side.id,
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            id,
            "collide",
            json!({
                "kind": FragmentKind::Idea.as_str(),
                "left": left.write(),
                "right": right.write(),
                "relation": relation.as_str(),
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        let collisions = self.spark_collisions(work_id, node_id, COLLISIONS_PER_BOARD)?;
        Ok(CollisionReceipt { spark_id: id, collisions })
    }

    /// 库里那些**已经成过对**的组合：认的是碰撞卡身上的两条溯源锚点。
    ///
    /// 一张卡上多于两条溯源锚点时只认前两条（v1 一次只撞两头；真有多源的那种，
    /// 组合它够不成"这一对撞过了"，宁可不跳过）。
    fn collided_pairs(&self, work_id: i64) -> Result<Vec<(CollisionSide, CollisionSide)>> {
        let mut stmt = self.conn.prepare(
            "SELECT linked FROM fragments
              WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map(params![work_id, FragmentKind::Idea.as_str()], |row| {
            row.get::<_, String>(0)
        })?;
        let mut out: Vec<(CollisionSide, CollisionSide)> = Vec::new();
        for row in rows {
            let sides: Vec<CollisionSide> = anchors_of(&row?)
                .iter()
                .filter_map(|anchor| CollisionSide::parse(anchor).ok())
                .collect();
            if sides.len() < 2 {
                continue;
            }
            let pair = canonical_order(sides[0], sides[1]);
            if pair.0 != pair.1 && !out.contains(&pair) {
                out.push(pair);
            }
        }
        Ok(out)
    }
}
