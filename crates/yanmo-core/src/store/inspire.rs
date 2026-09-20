//! 灵感那条线的读写：**当下最该被看见的碎片是哪几条**，以及把它落到哪儿去。
//!
//! 引力公式本身在 [`crate::gravity::inspire`]（纯逻辑、可假时钟单测）；这里只做三件事：
//! 把库里的行喂成候选、把候选按引力排出来、把"采纳 / 丢弃"这两笔账记进事务里。
//!
//! # 候选从哪来
//!
//! 就是**作者自己随手记的那几种**（灵感速记 / 事件 / 口述段落，`frag_kind` 见
//! [`FragmentKind::jotted`]）——问题卡与答案是叩问那条线的，它们有自己的处置面板，
//! 混进这里只会让"这条是谁的"说不清。**不新开表、不新加列**：引力要的四样
//! （`importance` / `created_at` / `used_count` / `linked`）碎片表里本来就有。
//!
//! # 「采纳」记的是哪笔账
//!
//! 采纳一次 = `used_count + 1` 且把"上次用掉"的时刻刷新——**新颖度**因此掉一截
//! （见 [`crate::gravity::novelty`]），同一条灵感就不会一直顶在最前面。
//! `status` 那一列**一个字都不动**：它是叩问六态生命周期在用的列，创作流这条线
//! 只借 `used_count` 这本账（要"用没用上"的话，这本账就是）。
//!
//! # 「丢弃」是软删
//!
//! 与作品 / 章节 / 碎片同一条纪律：删只是打时间戳，捞回就是把时间戳抹掉
//! （[`Store::restore_fragment`] 已经能捞）。

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use super::question_land::{check_landing_node, merged_outline};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::inspire::{rank, spark_gravity, SparkCandidate, SparkGravity, SparkParams};
use crate::model::{FragmentKind, NodeKind, SparkTarget};
use crate::question::anchors_of;
use crate::time::now_millis;

/// 一屏最多摆几条候选（按引力取头部；再多的碎片不是这一屏的事）。
pub const SPARKS_PER_BOARD: usize = 20;

/// 读候选时最多看多少条碎片——与碎片面板同一口径（[`super::FRAGMENTS_PER_BOARD`]）。
const SCAN_PER_KIND: usize = super::FRAGMENTS_PER_BOARD;

/// 这一条灵感的事实（库里那一行的原样；`kind` / `linked` 还没解析）。
struct RawSpark {
    id: i64,
    work_id: Option<i64>,
    kind: String,
    body: String,
    source: String,
    importance: f64,
    used_count: i64,
    /// 库里的 `last_asked_at`——在创作流这条线上它的语义是"上次被采纳的时刻"
    /// （叩问那条线用它是"上次被问出的时刻"，同一列、两本账各记各的）。
    last_used_at: Option<i64>,
    created_at: i64,
    linked: String,
}

const COLS: &str = "id, work_id, frag_kind, body, source, importance, used_count, last_asked_at, \
                    created_at, linked";

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSpark> {
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
    /// 被采纳过几次（前面那一档的账）
    pub used_count: i64,
    pub gravity: SparkGravity,
}

/// 采纳一条灵感的回执：落点 + 落到哪儿去了（正文那一段字由界面插）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdoptReceipt {
    pub spark_id: i64,
    pub target: SparkTarget,
    /// 落**正文**时要插进编辑器的那段字（其余落点是空串）
    pub text: String,
    /// 章纲最后成了什么（没落章纲就是 `None`）
    pub outline: Option<String>,
    /// 新建的场景卡 id（没落场景卡就是 `None`）
    pub scene_id: Option<i64>,
    /// 新建的事件碎片 id（没落事件池就是 `None`）
    pub event_id: Option<i64>,
    /// 采纳之后这一屏该是什么（界面直接换掉手上那份，不必自己猜）
    pub sparks: Vec<Spark>,
}

/// 当前焦点：几条**锚点**形态的"作者现在写到哪儿"。
///
/// 现在只知道"打开着哪一章"（与记碎片时那个 `chapter:<章 id>` 锚点是同一套写法）；
/// 将来要加"这一卷 / 在场人物"，只需往这个列表里多塞几条锚点——**引力公式一个字不改**。
fn focus_anchors(node_id: Option<i64>) -> Vec<String> {
    node_id.map(|id| vec![format!("chapter:{id}")]).unwrap_or_default()
}

/// 「这条不是随手记的那几种」：同一个说法只写一处（读一条、丢弃都用它）。
fn not_a_spark(id: i64) -> Error {
    Error::invalid_with(codes::SPARK_NOT_FOUND, [("spark_id", id.to_string())])
}

impl Store {
    /// 读一条灵感（**顺便把"它得是随手记的那几种"校验掉**）。
    fn spark_row(&self, id: i64) -> Result<RawSpark> {
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

    /// **灵感候选**：这本书里作者随手记的碎片，按引力排出前 `limit` 条。
    ///
    /// `node_id`＝作者现在打开着哪一章（给 `None` 就是"没落在某一章"，关联度一律 1.0，
    /// 排序退化成「重要度 × 热度 × 新颖度」）。**这个参数只影响排序，不影响谁能上场**。
    pub fn spark_candidates(
        &self,
        work_id: i64,
        node_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Spark>> {
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
            let rows = stmt.query_map(params![work_id, kind.as_str(), SCAN_PER_KIND as i64], read_row)?;
            for row in rows {
                let raw = row?;
                let candidate = SparkCandidate {
                    fragment_id: raw.id,
                    importance: raw.importance,
                    created_at: raw.created_at,
                    used_count: raw.used_count,
                    last_used_at: raw.last_used_at,
                    anchors: anchors_of(&raw.linked),
                };
                sparks.push(Spark {
                    id: raw.id,
                    kind,
                    body: raw.body,
                    source: raw.source,
                    anchors: candidate.anchors.clone(),
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
        let mut out: Vec<Spark> = ranked
            .into_iter()
            .filter_map(|(id, _)| by_id.remove(&id))
            .collect();
        out.truncate(limit);
        Ok(out)
    }

    /// **采纳一条灵感**：记下这本账，并按落点把它送到该去的地方（同一个事务）。
    ///
    /// 四种落点：
    /// - `body`：**核心不写正文**（击键级的写入留在壳内的编辑会话里）——回执里的 `text`
    ///   就是界面要插进去的那段字；
    /// - `outline`：合到这一章的"一句话"上（原有的在前，用「；」接，与叩问落章纲同一个实现）；
    /// - `scene`：在这一章下面新建一张场景卡，灵感就是它的正文；
    /// - `event`：新建一条**事件**碎片（原文一个字不动），`derived_from` 指回这条灵感。
    ///
    /// 允许反复采纳（同一条灵感既进章纲又进事件池是很正常的事）：每采纳一次记一笔，
    /// 新颖度跟着掉一截——**但绝不禁止**（作者说了算）。
    pub fn adopt_spark(
        &mut self,
        spark_id: i64,
        target: &str,
        node_id: Option<i64>,
        title: &str,
        trigger: &str,
    ) -> Result<AdoptReceipt> {
        let spark = self.spark_row(spark_id)?;
        let work_id = spark.work_id.unwrap_or(0);
        let target = SparkTarget::from_wire(target)?;
        if target == SparkTarget::Event && spark.kind == FragmentKind::Event.as_str() {
            return Err(Error::invalid_with(
                codes::SPARK_ALREADY_EVENT,
                [("spark_id", spark_id.to_string())],
            ));
        }
        // 落点那一章的校验与命名规则都在**事务外**先问好：事务里已经借着 `self.conn`，
        // 不能再借一次 `self`（见 `node_edit::create_node_in` 的说明）。
        let chapter = if target.needs_chapter() {
            let node = node_id.ok_or_else(|| {
                Error::invalid_with(
                    codes::SPARK_NEEDS_CHAPTER,
                    [("target", target.as_str().to_string())],
                )
            })?;
            check_landing_node(self, work_id, node)?;
            Some(node)
        } else {
            None
        };
        let outline = match target {
            SparkTarget::Outline => {
                merged_outline(self, chapter.unwrap_or(0), &[spark.body.clone()])?
            }
            _ => None,
        };
        let naming = self.naming_style(work_id)?;

        let now = now_millis();
        let tx = self.conn.transaction()?;
        // ① 章纲先落定（与这条灵感的账同一个事务）
        if let Some(text) = &outline {
            super::node_edit::set_summary_in(&tx, chapter.unwrap_or(0), text)?;
            Self::record_in(
                &self.device_id,
                &tx,
                "nodes",
                chapter.unwrap_or(0),
                "set_summary",
                json!({ "chars": text.chars().count(), "from_spark": spark_id }),
            )?;
        }
        // ② 场景卡：这一章下面新建一张，灵感就是它的正文（建卡与正文同一笔）
        let scene_id = match target {
            SparkTarget::Scene => {
                let parent = chapter.unwrap_or(0);
                let id = super::node_edit::create_node_in(
                    &tx,
                    work_id,
                    Some(parent),
                    NodeKind::Scene,
                    title,
                    naming,
                )?;
                let stats = super::content::stats_of(&spark.body);
                super::content::put_body(&tx, id, &spark.body, stats)?;
                Self::record_in(
                    &self.device_id,
                    &tx,
                    "nodes",
                    id,
                    "create",
                    json!({
                        "work_id": work_id,
                        "parent_id": parent,
                        "kind": NodeKind::Scene.as_str(),
                        "chars": stats.char_count,
                        "from_spark": spark_id,
                    }),
                )?;
                Some(id)
            }
            _ => None,
        };
        // ③ 事件池：新建一条事件碎片（**原文不动**），溯源指回这条灵感
        let event_id = match target {
            SparkTarget::Event => {
                tx.execute(
                    "INSERT INTO fragments(work_id, frag_kind, body, source, status, importance,
                                           used_count, last_asked_at, linked, derived_from,
                                           auto_derived, template_key, created_at, updated_at)
                     VALUES(?1, ?2, ?3, ?4, 'pending', ?5, 0, NULL, ?6, ?7, 0, '', ?8, ?8)",
                    params![
                        work_id,
                        FragmentKind::Event.as_str(),
                        spark.body,
                        spark.source,
                        spark.importance,
                        spark.linked,
                        spark_id,
                        now
                    ],
                )?;
                let id = tx.last_insert_rowid();
                Self::record_in(
                    &self.device_id,
                    &tx,
                    "fragments",
                    id,
                    "create",
                    json!({
                        "kind": FragmentKind::Event.as_str(),
                        "source": spark.source,
                        "from_spark": spark_id,
                        "trigger": trigger,
                    }),
                )?;
                Some(id)
            }
            _ => None,
        };
        // ④ 这条灵感的账：用过一次（新颖度掉一截）+ 一条痕
        tx.execute(
            "UPDATE fragments SET used_count = used_count + 1, last_asked_at = ?1, updated_at = ?1
              WHERE id = ?2 AND deleted_at IS NULL",
            params![now, spark_id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            spark_id,
            "adopt",
            json!({
                "target": target.as_str(),
                "node_id": chapter,
                "work_id": work_id,
                "scene_id": scene_id,
                "event_id": event_id,
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        let sparks = self.spark_candidates(work_id, node_id, SPARKS_PER_BOARD)?;
        Ok(AdoptReceipt {
            spark_id,
            target,
            // 正文那一段字回给界面去插（核心不碰编辑器手里那一份）
            text: if target == SparkTarget::Body { spark.body } else { String::new() },
            outline,
            scene_id,
            event_id,
            sparks,
        })
    }

    /// **丢弃一条灵感**：软删（捞回见 [`Store::restore_fragment`]）。
    ///
    /// 与"采纳"的分别：丢弃**不动新颖度那本账**——它不是"用掉了"，是"这条不要了"；
    /// 捞回来时它还是原来那条（`used_count` 一个字节没变）。
    pub fn discard_spark(
        &mut self,
        spark_id: i64,
        node_id: Option<i64>,
        trigger: &str,
    ) -> Result<Vec<Spark>> {
        let spark = self.spark_row(spark_id)?;
        let work_id = spark.work_id.unwrap_or(0);
        self.delete_fragment(spark_id, trigger)?;
        self.spark_candidates(work_id, node_id, SPARKS_PER_BOARD)
    }
}
