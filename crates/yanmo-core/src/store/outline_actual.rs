//! 「计划 vs 实际」要读的那点数据，以及确认之后的**只补不删**写入。
//!
//! # 分工
//!
//! 判据在 [`crate::outline::actual`]（纯函数：谁在正文里出现了、哪条伏笔像被点到了）；
//! 这里只干两件事：把值读齐（SQL 的形状只在这一处知道），以及**一个**写动作——
//! 把正文里出现、计划里没有的人补进这一章的出场人物。
//!
//! # 两条纪律
//!
//! 1. **读的那一半只读不写**：打开这一屏不该动一个字节；
//! 2. 写的那个动作**只补不删**，而且**改之前先留底**（[`Store::align_outline_cast`]）——
//!    "计划里列了、正文里没认到"只提示、绝不自动撤：删计划比加计划危险得多。

use rusqlite::{params, OptionalExtension};

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::SceneFields;
use crate::outline::{
    chapter_state, foreshadow_candidates, mentioned_cards, plan_is_empty, CastRef, ChapterActual,
};

/// 一次最多对到第几章（书太大时的安全阀；不静默——多出来的章数如实报给界面）。
pub const ACTUAL_MAX_CHAPTERS: usize = 2000;
/// 一章最多报几条"像被点到的伏笔"（多了这一屏就没法看了）。
const FORESHADOW_HITS_MAX: usize = 20;
/// 每一章留几份大纲留底（滚动；只留最近这几份）。
const OUTLINE_SNAPSHOTS_KEPT: i64 = 10;

/// 全书对一遍的结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OutlineActualReport {
    pub chapters: Vec<ChapterActual>,
    /// 这一屏之外还有几章没对到（书太大时才有；界面要如实说）
    pub truncated: usize,
}

/// 一份大纲留底的摘要（**不带 payload**：要看内容再拉）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OutlineSnapshotSummary {
    pub id: i64,
    pub node_id: i64,
    /// 为什么留的（稳定码：`align_cast`；界面字典渲染）
    pub note: String,
    pub created_at: i64,
}

/// 留底里存的那份大纲（JSON 的形状**只在这一处**定义）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OutlinePayload {
    version: u32,
    summary: String,
    /// 视角 / 目标 / 冲突 / 结果（顺序固定）
    fields: [String; 4],
    /// 出场人物的设定卡 id
    cast: Vec<i64>,
}

const PAYLOAD_VERSION: u32 = 1;

impl Store {
    /// 全书对一遍：每一章「计划 vs 实际」（**只读**）。
    ///
    /// 只对**承载正文的节点**（卷与分组行不在这一屏）；两样都空的章（没计划也没正文）
    /// 跳过——那是"还没轮到这一章"，报出来只会把真问题淹掉。
    pub fn outline_actuals(&self, work_id: i64) -> Result<OutlineActualReport> {
        let rows = self.outline_rows(work_id)?;
        // 设定卡与伏笔一次读全：每章各读一遍就把"这本书有多少卡"变成 N 次查询
        let cards = self.entity_cards(work_id, None)?;
        let foreshadows = self.foreshadows(work_id, None)?;

        let mut chapters = Vec::new();
        let mut seen = 0usize;
        for row in rows.into_iter().filter(|row| row.kind.holds_body()) {
            let plan_empty = plan_is_empty(
                &row.summary,
                &row.fields,
                row.cast.len(),
                row.planted_open + row.collected,
            );
            // 没计划也没正文：还没轮到这一章
            if plan_empty && !row.has_body {
                continue;
            }
            seen += 1;
            if seen > ACTUAL_MAX_CHAPTERS {
                break;
            }

            let planned: Vec<CastRef> = row
                .cast
                .iter()
                .map(|member| CastRef {
                    entity_id: member.entity_id,
                    name: member.name.clone(),
                    matched: member.name.clone(),
                })
                .collect();
            let (mentioned, mut hits) = if row.has_body {
                let body = self.read_body(row.node_id)?;
                (mentioned_cards(&body, &cards), foreshadow_candidates(&body, &foreshadows))
            } else {
                (Vec::new(), Vec::new())
            };
            hits.truncate(FORESHADOW_HITS_MAX);

            let matched: Vec<CastRef> = mentioned
                .iter()
                .filter(|found| planned.iter().any(|p| p.entity_id == found.entity_id))
                .cloned()
                .collect();
            let extra: Vec<CastRef> = mentioned
                .iter()
                .filter(|found| !planned.iter().any(|p| p.entity_id == found.entity_id))
                .cloned()
                .collect();
            let missing: Vec<CastRef> = planned
                .iter()
                .filter(|p| !mentioned.iter().any(|found| found.entity_id == p.entity_id))
                .cloned()
                .collect();

            let Some(state) =
                chapter_state(plan_empty, row.has_body, extra.len(), missing.len())
            else {
                continue;
            };
            chapters.push(ChapterActual {
                node_id: row.node_id,
                title: row.title,
                state,
                has_body: row.has_body,
                planned,
                matched,
                extra,
                missing,
                foreshadows: hits,
            });
        }

        let truncated = seen.saturating_sub(chapters.len());
        Ok(OutlineActualReport { chapters, truncated })
    }

    /// 这一章最近一份大纲留底（界面拿它决定"能不能撤销上一次对齐"）。
    pub fn latest_outline_snapshot(&self, node_id: i64) -> Result<Option<OutlineSnapshotSummary>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, node_id, note, created_at FROM outline_snapshots
                  WHERE node_id = ?1 ORDER BY id DESC LIMIT 1",
                params![node_id],
                |row| {
                    Ok(OutlineSnapshotSummary {
                        id: row.get(0)?,
                        node_id: row.get(1)?,
                        note: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// 这本书**每一章最近一份**大纲留底（有留底的章才在结果里）——界面拿它决定
    /// "这一章能不能撤销上一次对齐"。一次查询拿全，不必逐章问。
    pub fn outline_snapshot_index(&self, work_id: i64) -> Result<Vec<OutlineSnapshotSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_id, note, created_at FROM outline_snapshots
              WHERE work_id = ?1 AND id IN (
                    SELECT MAX(id) FROM outline_snapshots WHERE work_id = ?1 GROUP BY node_id
              )
              ORDER BY node_id",
        )?;
        let rows = stmt.query_map(params![work_id], |row| {
            Ok(OutlineSnapshotSummary {
                id: row.get(0)?,
                node_id: row.get(1)?,
                note: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 把这些人**补进**这一章的出场人物（只补不删），返回真正新增的个数。
    ///
    /// **改之前先留底**（[`Store::restore_outline_snapshot`] 能把这一章的大纲放回去）：
    /// 手滑一次也能退回来。
    pub fn align_outline_cast(&mut self, node_id: i64, entity_ids: &[i64]) -> Result<usize> {
        self.node_work(node_id)?; // 节点不在就明确报错
        let before: Vec<i64> = self.node_cast_of(node_id)?.into_iter().map(|m| m.entity_id).collect();
        // 只补：把要补的并进现有那一份，现有的一个都不动
        let mut merged: Vec<i64> = before.clone();
        let mut added = 0usize;
        for id in entity_ids {
            if !merged.contains(id) {
                merged.push(*id);
                added += 1;
            }
        }
        if added == 0 {
            return Ok(0); // 全是已有的：不留底、不写库（点了没变就不该留痕迹）
        }
        self.snapshot_outline(node_id, "align_cast")?;
        let trigger = "outline_align";
        self.set_node_cast(node_id, &merged, trigger)?;
        Ok(added)
    }

    /// 回滚到某一份大纲留底（撤销上一次对齐）。
    ///
    /// 与"对齐"不同，这一步**会**把这一章的大纲整体写成留底里那一份——它本来就是
    /// "撤销"：改动前的样子是什么就是什么。
    pub fn restore_outline_snapshot(&mut self, snapshot_id: i64) -> Result<i64> {
        let (node_id, payload): (i64, String) = self
            .conn
            .query_row(
                "SELECT node_id, payload FROM outline_snapshots WHERE id = ?1",
                params![snapshot_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(
                    codes::SNAPSHOT_NOT_FOUND,
                    [("snapshot_id", snapshot_id.to_string())],
                )
            })?;
        let saved: OutlinePayload = serde_json::from_str(&payload)
            .map_err(|_| Error::invalid(codes::SNAPSHOT_NOT_FOUND))?;

        // 章纲（一句话）
        self.set_node_summary(node_id, &saved.summary)?;
        // 四格
        let fields = SceneFields {
            node_id,
            pov: saved.fields[0].clone(),
            goal: saved.fields[1].clone(),
            conflict: saved.fields[2].clone(),
            outcome: saved.fields[3].clone(),
        };
        self.save_scene_fields(&fields, "outline_restore")?;
        // 出场人物：留底里是什么就是什么（撤销的意义就在这儿）
        self.set_node_cast(node_id, &saved.cast, "outline_restore")?;
        Ok(node_id)
    }

    /// 把这一章现在的大纲写成一份留底（内部：只有一个动作会调它）。
    fn snapshot_outline(&mut self, node_id: i64, note: &str) -> Result<i64> {
        let work_id = self.node_work(node_id)?;
        let summary: String = self
            .conn
            .query_row("SELECT summary FROM nodes WHERE id = ?1", params![node_id], |row| {
                row.get(0)
            })?;
        let fields = self.scene_fields(node_id)?;
        let cast: Vec<i64> = self.node_cast_of(node_id)?.into_iter().map(|m| m.entity_id).collect();
        let payload = OutlinePayload {
            version: PAYLOAD_VERSION,
            summary,
            fields: [fields.pov, fields.goal, fields.conflict, fields.outcome],
            cast,
        };
        let text = serde_json::to_string(&payload)
            .map_err(|_| Error::invalid(codes::SNAPSHOT_NOT_FOUND))?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO outline_snapshots(node_id, work_id, payload, note, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![node_id, work_id, text, note, crate::time::now_millis()],
        )?;
        let id = tx.last_insert_rowid();
        // 滚动保留：每章只留最近几份
        tx.execute(
            "DELETE FROM outline_snapshots
              WHERE node_id = ?1 AND id NOT IN (
                    SELECT id FROM outline_snapshots WHERE node_id = ?1 ORDER BY id DESC LIMIT ?2
              )",
            params![node_id, OUTLINE_SNAPSHOTS_KEPT],
        )?;
        tx.commit()?;
        Ok(id)
    }
}
