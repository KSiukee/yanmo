//! 灵感的**落笔**：采纳到哪儿去、丢弃怎么算。
//!
//! 与 [`super::inspire`]（读数）分开的原因：那半边管"先看哪条"，这半边管"这条用掉了"——
//! 变化理由不一样（一个跟着引力公式长，一个跟着落点长），与"问题卡的读写与状态迁移分开"
//! 是同一条分工。
//!
//! # 四种落点
//!
//! - `body`：**核心不写正文**（击键级的写入留在壳内的编辑会话里）——回执里的 `text`
//!   就是界面要插进去的那段字；
//! - `outline`：合到这一章的"一句话"上（原有的在前，用「；」接，与叩问落章纲同一个实现）；
//! - `scene`：在这一章下面新建一张场景卡，灵感就是它的正文；
//! - `event`：新建一条**事件**碎片（原文一个字不动），`derived_from` 指回这条灵感。
//!
//! # 「采纳」记的是哪笔账
//!
//! 采纳一次 = `used_count + 1` 且把"上次用掉"的时刻刷新——**新颖度**因此掉一截
//! （见 [`crate::gravity::novelty`]），同一条灵感就不会一直顶在最前面。
//! `status` 那一列**一个字都不动**：它是叩问六态生命周期在用的列，创作流这条线
//! 只借 `used_count` 这本账（要"用没用上"的话，这本账就是）。
//!
//! 「丢弃」是**软删**（与作品 / 章节 / 碎片同一条纪律：删只是打时间戳，捞回就是把
//! 时间戳抹掉），而且**不动那本账**——丢弃是"这条不要了"，不是"用掉了"。

use rusqlite::params;
use serde::Serialize;
use serde_json::json;

use super::inspire::{Spark, SPARKS_PER_BOARD};
use super::question_land::{check_landing_node, merged_outline};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{FragmentKind, NodeKind, SparkTarget};
use crate::time::now_millis;

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

impl Store {
    /// **采纳一条灵感**：记下这本账，并按落点把它送到该去的地方（同一个事务）。
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
            text: if target == SparkTarget::Body {
                spark.body
            } else {
                String::new()
            },
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
