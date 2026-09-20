//! 大纲元素的**读法**：把章纲 / 伏笔 / 人物卡摊成"碰撞的一头"。
//!
//! 碰撞那一层的规则只认三样：**标识 + 分量 + 关联锚点**。碎片天生有这三样；
//! 三种大纲元素在这里各自摊一遍，于是它们能直接进同一套挑对规则，
//! **不需要第二套机制**（这是这一层存在的全部理由）。
//!
//! | 元素 | 标识 | 那段字 | 关联锚点 |
//! |---|---|---|---|
//! | 章纲 | `plan:<章 id>` | 这一章的"一句话" | 它自己那一章：`chapter:<章 id>` |
//! | 伏笔 | `foreshadow:<id>` | 伏笔正文 | 它**埋在哪一章** |
//! | 人物卡 | `entity:<id>` | 名字 | 它**出场过的每一章**（靠出场表） |
//!
//! # 四处分寸
//!
//! 1. **分量一律中性满分 1.0**：元素没有"作者标的重要度"，也没有"被用过几次"的账——
//!    机制**不该替它编一个**。1.0 的含义是"元素这一头不拉低分数"：配对分数因此等于
//!    碎片那一头的引力，也就是**碎片越靠前，它跟元素的配对也越靠前**。
//! 2. **章纲按"离当前这一章多近"取前 [`ELEMENT_POOL`] 条**：手边的章纲更值得撞；
//!    没打开章时按新章在前（同一句 SQL 靠 `ABS(id - NULL)` 全为 NULL 自然落到第二个排序键）。
//! 3. **伏笔只取还埋着的**（`planted`）：收了的是"已经了结的线"，不写了的是作者明确
//!    放下的——与大纲体检那条"只看 planted"的立场一致，不催办。
//! 4. **认不出的当没有**：元素被删了 / 章纲被清空了，`element_by_side` 返回 `None`，
//!    调用方如实报"这条不在了"，不猜一个近似的。

use rusqlite::{params, OptionalExtension};

use super::Store;
use crate::error::Result;
use crate::model::{CollisionKind, CollisionSide};

/// 每种元素最多取几条上场（碎片那一头另有自己的池子上限）。
pub const ELEMENT_POOL: usize = 40;

/// 一处大纲元素摊成的那一头：标识 + 锚点 + 要嵌进引子的那段字（+ 人物卡的备注）。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Element {
    pub(super) side: CollisionSide,
    pub(super) anchors: Vec<String>,
    /// 要嵌进引子的那段字：碎片 / 章纲 / 伏笔是正文，人物卡是名字
    pub(super) text: String,
    /// 人物卡的备注（其余为空串）——作者的补充说明，界面单独摆一行
    pub(super) note: String,
}

/// 章的关联锚点（与碎片那条线**同一个写法**：`chapter:<节点 id>`）。
///
/// 这是"元素能跟碎片撞起来"的接口：两边都摊成同一套锚点，共用判定就自然成立，
/// 不需要为元素另写一套"在不在同一章"的判断。
fn chapter_anchor(node_id: i64) -> String {
    format!("chapter:{node_id}")
}

impl Store {
    /// 把这本书的三种大纲元素都摊成碰撞的一头。
    pub(super) fn outline_elements(
        &self,
        work_id: i64,
        node_id: Option<i64>,
    ) -> Result<Vec<Element>> {
        let mut out = self.plan_elements(work_id, node_id)?;
        out.extend(self.foreshadow_elements(work_id)?);
        out.extend(self.entity_elements(work_id)?);
        Ok(out)
    }

    /// 按标识认回**一处**元素（保存碰撞时要校验 + 取锚点）；认不出给 `None`。
    pub(super) fn element_by_side(
        &self,
        work_id: i64,
        side: CollisionSide,
    ) -> Result<Option<Element>> {
        match side.kind {
            CollisionKind::Spark => Ok(None),
            CollisionKind::Plan => self.plan_by_id(work_id, side.id),
            CollisionKind::Foreshadow => self.foreshadow_by_id(work_id, side.id),
            CollisionKind::Entity => self.entity_by_id(work_id, side.id),
        }
    }

    /// 章纲：有"一句话"的那些章，**离当前这一章最近的先上场**。
    fn plan_elements(&self, work_id: i64, node_id: Option<i64>) -> Result<Vec<Element>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, summary FROM nodes
              WHERE work_id = ?1 AND deleted_at IS NULL AND TRIM(summary) <> ''
              ORDER BY ABS(id - ?2), id DESC
              LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![work_id, node_id, ELEMENT_POOL as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, summary) = row?;
            out.push(plan_element(id, summary));
        }
        Ok(out)
    }

    fn plan_by_id(&self, work_id: i64, id: i64) -> Result<Option<Element>> {
        let summary: Option<String> = self
            .conn
            .query_row(
                "SELECT summary FROM nodes
                  WHERE id = ?1 AND work_id = ?2 AND deleted_at IS NULL AND TRIM(summary) <> ''",
                params![id, work_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(summary.map(|text| plan_element(id, text)))
    }

    /// 伏笔：只取**还埋着**的，锚在它埋下的那一章。
    fn foreshadow_elements(&self, work_id: i64) -> Result<Vec<Element>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, planted_node FROM foreshadows
              WHERE work_id = ?1 AND deleted_at IS NULL AND state = 'planted'
              ORDER BY id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![work_id, ELEMENT_POOL as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, body, planted) = row?;
            out.push(foreshadow_element(id, body, planted));
        }
        Ok(out)
    }

    fn foreshadow_by_id(&self, work_id: i64, id: i64) -> Result<Option<Element>> {
        let found: Option<(String, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT body, planted_node FROM foreshadows
                  WHERE id = ?1 AND work_id = ?2 AND deleted_at IS NULL AND state = 'planted'",
                params![id, work_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(found.map(|(body, planted)| foreshadow_element(id, body, planted)))
    }

    /// 人物卡 / 设定卡：锚点是**它出场过的每一章**（靠出场表）。
    fn entity_elements(&self, work_id: i64) -> Result<Vec<Element>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, note FROM entity_cards
              WHERE work_id = ?1 AND deleted_at IS NULL
              ORDER BY id DESC
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![work_id, ELEMENT_POOL as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, note) = row?;
            out.push(Element {
                side: CollisionSide::new(CollisionKind::Entity, id),
                anchors: self.entity_anchors(id)?,
                text: name,
                note,
            });
        }
        Ok(out)
    }

    fn entity_by_id(&self, work_id: i64, id: i64) -> Result<Option<Element>> {
        let found: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT name, note FROM entity_cards
                  WHERE id = ?1 AND work_id = ?2 AND deleted_at IS NULL",
                params![id, work_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match found {
            None => Ok(None),
            Some((name, note)) => Ok(Some(Element {
                side: CollisionSide::new(CollisionKind::Entity, id),
                anchors: self.entity_anchors(id)?,
                text: name,
                note,
            })),
        }
    }

    /// 一张卡出场过哪几章（`node_cast` 那两列就是为这件事存的）。
    fn entity_anchors(&self, entity_id: i64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT node_id FROM node_cast WHERE entity_id = ?1 ORDER BY node_id")?;
        let rows = stmt.query_map(params![entity_id], |row| row.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(chapter_anchor(row?));
        }
        Ok(out)
    }
}

fn plan_element(id: i64, text: String) -> Element {
    Element {
        side: CollisionSide::new(CollisionKind::Plan, id),
        anchors: vec![chapter_anchor(id)],
        text,
        note: String::new(),
    }
}

fn foreshadow_element(id: i64, text: String, planted_node: Option<i64>) -> Element {
    Element {
        side: CollisionSide::new(CollisionKind::Foreshadow, id),
        // 没记埋在哪儿 = 没有锚点（那它只会以"相隔"的姿态上场，不慌）
        anchors: planted_node.map(chapter_anchor).into_iter().collect(),
        text,
        note: String::new(),
    }
}
