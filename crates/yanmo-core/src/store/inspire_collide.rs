//! 灵感的**碰撞**：哪两条碎片值得放一起、撞出来的那句怎么存、不想要的怎么跳过。
//!
//! 挑对的规则是纯逻辑（[`crate::gravity::collide`]）；这里只做三件事：把池子喂成事实、
//! 把"撞过的"和"跳过的"两组组合读出来、把作者写下的那句存成一张**双源溯源**的灵感卡。
//!
//! # 一条碰撞卡长什么样
//!
//! 它就是一张普通的灵感卡（`frag_kind = 'idea'`，**不新开表、不新加列**），只是身上多两样：
//!
//! - `derived_from` 指**主源**（两条里 id 小的那条）；
//! - `linked` 里除了两条的锚点，还多两条溯源锚点 `spark:<id>`——**第二条源头就靠它**，
//!   于是"这条是哪两条撞出来的"永远查得到（与"记灵感"那条 `derived_from` 的讲究一致）。
//!
//! 那两条 `spark:` 锚点顺手当了**去重账**：下一轮挑对时，凡是已经这样成过对的组合都不再撞。
//!
//! # 「跳过」为什么要落盘
//!
//! 碰撞是**每次打开都重算**的（池子一直在变），所以"这一对我看过了、不想撞"必须记下来，
//! 否则同一对会永远摆在最前面——被无视的提醒等于没有提醒（与大纲那条忽略标记同一个道理，
//! 也住在 `settings` 里：`work.{id}.spark.collision.skipped`，一把不透明记号，整把读写）。

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use super::inspire::Spark;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::collide::{
    order, pick_pairs, relation_of, CollideParams, CollisionFact, CollisionRelation,
};
use crate::model::{FragmentKind, InputSource};
use crate::question::anchors_of;
use crate::time::now_millis;

/// 一屏摆几对（再多就不是"撞一下"，是让你干活了）。
pub const COLLISIONS_PER_BOARD: usize = 3;

/// 参与挑对的池子上限——只让引力靠前的这些上场：
/// 再往后的碎片本来就排不进"此刻该看"，也省得把组合数算成平方级的浪费。
const COLLIDE_POOL: usize = 60;

/// 一对碰撞建议：两条碎片本身 + 关系 + 分数（界面拿关系码渲染一句引子）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Collision {
    pub left: Spark,
    pub right: Spark,
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

/// 这本书"跳过"的那些组合存在哪个键下（按书分：换一本书是另一套）。
fn skip_key(work_id: i64) -> String {
    format!("work.{work_id}.spark.collision.skipped")
}

/// 一条记号怎么写（**规范写法**：小 id 在前，与挑对那一层同一个口径）。
fn mark(pair: (i64, i64)) -> String {
    format!("{}:{}", pair.0, pair.1)
}

/// 记号怎么读回来；认不出的当没有（坏记录不该让整条链路转不动）。
fn parse_mark(text: &str) -> Option<(i64, i64)> {
    let (left, right) = text.split_once(':')?;
    let (left, right) = (left.trim().parse().ok()?, right.trim().parse().ok()?);
    Some(order(left, right))
}

impl Store {
    /// **一屏碰撞建议**：按规则挑几对，并把两边的碎片本身读出来。
    pub fn spark_collisions(
        &self,
        work_id: i64,
        node_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Collision>> {
        let pool: Vec<Spark> = self
            .scored_sparks(work_id, node_id)?
            .into_iter()
            .take(COLLIDE_POOL)
            .collect();
        let facts: Vec<CollisionFact> = pool
            .iter()
            .map(|spark| CollisionFact {
                fragment_id: spark.id,
                total: spark.gravity.total,
                anchors: spark.anchors.clone(),
            })
            .collect();
        // 撞过的（已经成过对的）与作者按过"跳过"的，都不再上场
        let mut seen = self.collided_pairs(work_id)?;
        for pair in self.skipped_collisions(work_id)? {
            if !seen.contains(&pair) {
                seen.push(pair);
            }
        }
        let pairs = pick_pairs(&facts, &seen, limit, &CollideParams::default());
        let mut by_id: HashMap<i64, Spark> = pool.into_iter().map(|spark| (spark.id, spark)).collect();
        Ok(pairs
            .into_iter()
            .filter_map(|pair| {
                let left = by_id.remove(&pair.left)?;
                let right = by_id.remove(&pair.right)?;
                Some(Collision {
                    left,
                    right,
                    relation: pair.relation,
                    shared_anchors: pair.shared_anchors,
                    score: pair.score,
                })
            })
            .collect())
    }

    /// **存下撞出来的那一句**：写成一张双源溯源的灵感卡（写卡 + 留痕，同一个事务）。
    ///
    /// 关系（同处 / 相隔）**由核心自己算**，不要调用方告诉它——那是这一层的事实，
    /// 让界面报一遍只会多一个能说岔的地方。它进留痕，于是"这条是怎么撞出来的"看得见。
    pub fn save_collision(
        &mut self,
        left: i64,
        right: i64,
        body: &str,
        trigger: &str,
    ) -> Result<CollisionReceipt> {
        let body = body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::IDEA_BODY_EMPTY));
        }
        let (left, right) = order(left, right);
        if left == right {
            return Err(Error::invalid_with(
                codes::COLLISION_SAME_SPARK,
                [("spark_id", left.to_string())],
            ));
        }
        let a = self.spark_row(left)?;
        let b = self.spark_row(right)?;
        let work_id = a.work_id.unwrap_or(0);
        if b.work_id.unwrap_or(0) != work_id {
            return Err(Error::invalid_with(
                codes::COLLISION_FOREIGN,
                [("left", left.to_string()), ("right", right.to_string())],
            ));
        }
        let anchors_a = anchors_of(&a.linked);
        let anchors_b = anchors_of(&b.linked);
        let (relation, _) = relation_of(&anchors_a, &anchors_b);
        // 新卡的锚点：两边的锚点合集 + 两条**溯源锚点**（第二条源头就靠它）
        let mut anchors = anchors_a;
        for anchor in anchors_b {
            if !anchors.contains(&anchor) {
                anchors.push(anchor);
            }
        }
        for id in [left, right] {
            let tag = format!("spark:{id}");
            if !anchors.contains(&tag) {
                anchors.push(tag);
            }
        }
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
                left,
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
                "left": left,
                "right": right,
                "relation": relation.as_str(),
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        let collisions = self.spark_collisions(work_id, None, COLLISIONS_PER_BOARD)?;
        Ok(CollisionReceipt { spark_id: id, collisions })
    }

    /// 这本书**跳过**的那些组合（规范写法；坏记录当没有）。
    pub fn skipped_collisions(&self, work_id: i64) -> Result<Vec<(i64, i64)>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![skip_key(work_id)],
                |row| row.get(0),
            )
            .optional()?;
        let marks: Vec<String> = raw
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default();
        let mut out = Vec::new();
        for text in marks {
            if let Some(pair) = parse_mark(&text) {
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
        Ok(out)
    }

    /// **跳过这一对**（幂等），返回跳完之后那一屏碰撞建议。
    pub fn skip_collision(
        &mut self,
        work_id: i64,
        left: i64,
        right: i64,
        node_id: Option<i64>,
        trigger: &str,
    ) -> Result<Vec<Collision>> {
        let pair = order(left, right);
        let mut current = self.skipped_collisions(work_id)?;
        if !current.contains(&pair) {
            current.push(pair);
            self.write_skips(work_id, &current, "skip", mark(pair), trigger)?;
        }
        self.spark_collisions(work_id, node_id, COLLISIONS_PER_BOARD)
    }

    /// **撤销一次跳过**（回头路：跳错了要能捡回来）。
    pub fn unskip_collision(
        &mut self,
        work_id: i64,
        left: i64,
        right: i64,
        node_id: Option<i64>,
        trigger: &str,
    ) -> Result<Vec<Collision>> {
        let pair = order(left, right);
        let kept: Vec<(i64, i64)> = self
            .skipped_collisions(work_id)?
            .into_iter()
            .filter(|item| *item != pair)
            .collect();
        self.write_skips(work_id, &kept, "unskip", mark(pair), trigger)?;
        self.spark_collisions(work_id, node_id, COLLISIONS_PER_BOARD)
    }

    /// 库里那些**已经成过对**的组合：认的是碰撞卡身上的两条 `spark:` 溯源锚点。
    ///
    /// 一张卡上多于两条溯源锚点时只认前两条（v1 一次只撞两条；真有多源的那种，
    /// 组合它够不成"这一对撞过了"，宁可不跳过）。
    fn collided_pairs(&self, work_id: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT linked FROM fragments
              WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map(params![work_id, FragmentKind::Idea.as_str()], |row| {
            row.get::<_, String>(0)
        })?;
        let mut out: Vec<(i64, i64)> = Vec::new();
        for row in rows {
            let ids: Vec<i64> = anchors_of(&row?)
                .iter()
                .filter_map(|anchor| {
                    anchor.strip_prefix("spark:").and_then(|value| value.trim().parse::<i64>().ok())
                })
                .collect();
            if ids.len() < 2 {
                continue;
            }
            let pair = order(ids[0], ids[1]);
            if pair.0 != pair.1 && !out.contains(&pair) {
                out.push(pair);
            }
        }
        Ok(out)
    }

    /// 落盘一把跳过记号 + 留痕（同一个事务；空清单就把键删掉，与偏好那条规矩一致）。
    fn write_skips(
        &mut self,
        work_id: i64,
        pairs: &[(i64, i64)],
        op: &str,
        pair: String,
        trigger: &str,
    ) -> Result<()> {
        let now = now_millis();
        let tx = self.conn.transaction()?;
        if pairs.is_empty() {
            tx.execute("DELETE FROM settings WHERE key = ?1", params![skip_key(work_id)])?;
        } else {
            let marks: Vec<String> = pairs.iter().map(|item| mark(*item)).collect();
            let value = serde_json::to_string(&marks).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![skip_key(work_id), value, now],
            )?;
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id,
            op,
            json!({ "pair": pair, "count": pairs.len(), "trigger": trigger }),
        )?;
        tx.commit()?;
        Ok(())
    }
}
