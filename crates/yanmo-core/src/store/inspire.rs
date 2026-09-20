//! 灵感那条线的**读数**：一池碎片里，此刻最该被看见的是哪几条。
//!
//! 引力公式本身在 [`crate::gravity::inspire`]（纯逻辑、可假时钟单测）；这里只做三件事：
//! 把库里的行喂成候选、把候选按引力排出来、把这一屏交出去。
//! **落笔那半边**（采纳到正文 / 章纲 / 场景卡 / 事件池）在 [`super::inspire_adopt`]，
//! **撞一下那半边**（挑对与跳过）在 [`super::inspire_collide`]——三件的读者与变化理由各不相同。
//!
//! # 候选从哪来
//!
//! 就是**作者自己随手记的那几种**（灵感速记 / 事件 / 口述段落，`frag_kind` 见
//! [`FragmentKind::jotted`]）——问题卡与答案是叩问那条线的，它们有自己的处置面板，
//! 混进这里只会让"这条是谁的"说不清。**不新开表、不新加列**：引力要的四样
//! （`importance` / `created_at` / `used_count` / `linked`）碎片表里本来就有。

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::inspire::{rank, spark_gravity, SparkCandidate, SparkGravity, SparkParams};
use crate::model::FragmentKind;
use crate::question::anchors_of;
use crate::time::now_millis;

/// 一屏最多摆几条候选（按引力取头部；再多的碎片不是这一屏的事）。
pub const SPARKS_PER_BOARD: usize = 20;

/// 读候选时最多看多少条碎片——与碎片面板同一口径（[`super::FRAGMENTS_PER_BOARD`]）。
const SCAN_PER_KIND: usize = super::FRAGMENTS_PER_BOARD;

/// 这一条灵感的事实（库里那一行的原样；`kind` / `linked` 还没解析）。
pub(super) struct RawSpark {
    pub(super) id: i64,
    pub(super) work_id: Option<i64>,
    pub(super) kind: String,
    pub(super) body: String,
    pub(super) source: String,
    pub(super) importance: f64,
    pub(super) used_count: i64,
    /// 库里的 `last_asked_at`——在创作流这条线上它的语义是"上次被采纳的时刻"
    /// （叩问那条线用它是"上次被问出的时刻"，同一列、两本账各记各的）。
    pub(super) last_used_at: Option<i64>,
    pub(super) created_at: i64,
    pub(super) linked: String,
}

pub(super) const COLS: &str = "id, work_id, frag_kind, body, source, importance, used_count, \
                               last_asked_at, created_at, linked";

pub(super) fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSpark> {
    Ok(RawSpark {
        id: row.get(0)?,
        work_id: row.get(1)?,
        kind: row.get(2)?,
        body: row.get(3)?,
        source: row.get(4)?,
        importance: row.get(5)?,
        used_count: row.get(6)?,
        last_used_at: row.get(7)?,
        created_at: row.get(8)?,
        linked: row.get(9)?,
    })
}

/// 一条灵感候选：碎片本身 + 它的引力拆解（界面拿拆解回答"为什么先看它"）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spark {
    pub id: i64,
    pub kind: FragmentKind,
    pub body: String,
    /// 怎么记下的（`typed` / `voice` / `mixed`）——采纳到事件池时照抄它
    pub source: String,
    pub anchors: Vec<String>,
    pub importance: f64,
    pub created_at: i64,
    /// 被采纳过几次（采纳那一档的账）
    pub used_count: i64,
    pub gravity: SparkGravity,
}

/// 当前焦点：几条**锚点**形态的"作者现在写到哪儿"。
///
/// 现在只知道"打开着哪一章"（与记碎片时那个 `chapter:<章 id>` 锚点是同一套写法）；
/// 将来要加"这一卷 / 在场人物"，只需往这个列表里多塞几条锚点——**引力公式一个字不改**。
pub(super) fn focus_anchors(node_id: Option<i64>) -> Vec<String> {
    node_id
        .map(|id| vec![format!("chapter:{id}")])
        .unwrap_or_default()
}

/// 「这条不是随手记的那几种」：同一个说法只写一处（读一条、采纳、丢弃都用它）。
pub(super) fn not_a_spark(id: i64) -> Error {
    Error::invalid_with(codes::SPARK_NOT_FOUND, [("spark_id", id.to_string())])
}

impl Store {
    /// 读一条灵感（**顺便把"它得是随手记的那几种"校验掉**）。
    pub(super) fn spark_row(&self, id: i64) -> Result<RawSpark> {
        let sql = format!("SELECT {COLS} FROM fragments WHERE id = ?1 AND deleted_at IS NULL");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_row)
            .optional()?
            .ok_or_else(|| not_a_spark(id))?;
        let kind = FragmentKind::parse(&raw.kind)?;
        if !kind.jotted() {
            return Err(not_a_spark(id));
        }
        Ok(raw)
    }

    /// **全部**候选：这本书里作者随手记的碎片，按引力排好序（不截断）。
    ///
    /// 一屏只摆前几条（[`SPARKS_PER_BOARD`]），但**撞一下要看池子**（挑对要足够的组合），
    /// 所以排序与截断分成两步。
    pub(super) fn scored_sparks(&self, work_id: i64, node_id: Option<i64>) -> Result<Vec<Spark>> {
        let focus = focus_anchors(node_id);
        let now = now_millis();
        let p = SparkParams::default();
        let sql = format!(
            "SELECT {COLS} FROM fragments
              WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
              ORDER BY created_at DESC, id DESC
              LIMIT ?3"
        );
        let mut sparks = Vec::new();
        for kind in FragmentKind::JOTTED {
            let mut stmt = self.conn.prepare(&sql)?;
            let rows =
                stmt.query_map(params![work_id, kind.as_str(), SCAN_PER_KIND as i64], read_row)?;
            for row in rows {
                let raw = row?;
                let anchors = anchors_of(&raw.linked);
                let candidate = SparkCandidate {
                    fragment_id: raw.id,
                    importance: raw.importance,
                    created_at: raw.created_at,
                    used_count: raw.used_count,
                    last_used_at: raw.last_used_at,
                    anchors: anchors.clone(),
                };
                sparks.push(Spark {
                    id: raw.id,
                    kind,
                    body: raw.body,
                    source: raw.source,
                    anchors,
                    importance: candidate.importance,
                    created_at: candidate.created_at,
                    used_count: candidate.used_count,
                    gravity: spark_gravity(&candidate, &focus, now, &p),
                });
            }
        }
        // 排序规则只有引力那一层知道（`rank`）：这一层只把结果按 id 摆回碎片形状，
        // 免得"同分怎么排"在两处各写一份。
        let ranked = rank(sparks.iter().map(|s| (s.id, s.gravity.clone())).collect());
        let mut by_id: HashMap<i64, Spark> = sparks.into_iter().map(|s| (s.id, s)).collect();
        Ok(ranked
            .into_iter()
            .filter_map(|(id, _)| by_id.remove(&id))
            .collect())
    }

    /// **灵感候选**：按引力排出前 `limit` 条（这一屏就这么多）。
    ///
    /// `node_id`＝作者现在打开着哪一章（给 `None` 就是"没落在某一章"，关联度一律 1.0，
    /// 排序退化成「重要度 × 热度 × 新颖度」）。**这个参数只影响排序，不影响谁能上场**。
    pub fn spark_candidates(
        &self,
        work_id: i64,
        node_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Spark>> {
        let mut sparks = self.scored_sparks(work_id, node_id)?;
        sparks.truncate(limit);
        Ok(sparks)
    }
}
