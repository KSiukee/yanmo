//! 碰撞的**跳过**：作者说"这一对我看过了、不想撞"的那本账。
//!
//! 与 [`super::inspire_collide`]（摆出这一屏）分开的原因：那边管"挑哪几对"，
//! 这边管"哪几对别再挑"——两件事的变化理由不一样（一个跟着规则长，一个跟着处置长）。
//!
//! # 为什么落盘
//!
//! 碰撞是**每次打开都重算**的（池子一直在变），所以不记下来的话，同一对会永远摆在最前面——
//! 被无视的提醒等于没有提醒（与大纲那条忽略标记同一个道理）。它住在 `settings` 里：
//! `work.{id}.spark.collision.skipped`，一把不透明记号，整把读写；键按书分。
//!
//! 一条记号的写法是**规范写法**：`spark:12|plan:3`（小的那一头在前，与挑对那一层同一个口径），
//! 认不出的当没有——坏记录不该让整条链路转不动。
//!
//! 还有一条分寸：跳过**不算"撞过"**。它只是"这一对先别摆"，撤销之后原样回来；
//! 真正算"撞过"的是那些**已经写下的碰撞卡**（它们身上刻着两头的溯源，见
//! [`super::inspire_collide`]）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::inspire_collide::{Collision, COLLISIONS_PER_BOARD};
use super::Store;
use crate::error::Result;
use crate::model::{canonical_order, CollisionSide};
use crate::time::now_millis;

/// 这本书"跳过"的那些组合存在哪个键下（按书分：换一本书是另一套）。
fn skip_key(work_id: i64) -> String {
    format!("work.{work_id}.spark.collision.skipped")
}

/// 一条记号怎么写（**规范写法**：小的一头在前，与挑对那一层同一个口径）。
fn mark(pair: (CollisionSide, CollisionSide)) -> String {
    format!("{}|{}", pair.0.write(), pair.1.write())
}

/// 记号怎么读回来；认不出的当没有（坏记录不该让整条链路转不动）。
fn parse_mark(text: &str) -> Option<(CollisionSide, CollisionSide)> {
    let (left, right) = text.split_once('|')?;
    let (left, right) = (CollisionSide::parse(left).ok()?, CollisionSide::parse(right).ok()?);
    Some(canonical_order(left, right))
}

impl Store {
    /// 这本书**跳过**的那些组合（规范写法；坏记录当没有）。
    pub fn skipped_collisions(&self, work_id: i64) -> Result<Vec<(CollisionSide, CollisionSide)>> {
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
        left: &str,
        right: &str,
        node_id: Option<i64>,
        trigger: &str,
    ) -> Result<Vec<Collision>> {
        let pair = canonical_order(CollisionSide::parse(left)?, CollisionSide::parse(right)?);
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
        left: &str,
        right: &str,
        node_id: Option<i64>,
        trigger: &str,
    ) -> Result<Vec<Collision>> {
        let pair = canonical_order(CollisionSide::parse(left)?, CollisionSide::parse(right)?);
        let kept: Vec<(CollisionSide, CollisionSide)> = self
            .skipped_collisions(work_id)?
            .into_iter()
            .filter(|item| *item != pair)
            .collect();
        self.write_skips(work_id, &kept, "unskip", mark(pair), trigger)?;
        self.spark_collisions(work_id, node_id, COLLISIONS_PER_BOARD)
    }

    /// 落盘一把跳过记号 + 留痕（同一个事务；空清单就把键删掉，与偏好那条规矩一致）。
    fn write_skips(
        &mut self,
        work_id: i64,
        pairs: &[(CollisionSide, CollisionSide)],
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
