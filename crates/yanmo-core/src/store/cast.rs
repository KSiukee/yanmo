//! 出场人物：**节点（章 / 节 / 场景卡）↔ 设定卡**的关联。
//!
//! 表与理由见 `db::migrations_v13`。这里管三件事：读一本书的、读一段的、**换掉一段的**。
//!
//! 四条分寸：
//!
//! 1. **整份覆盖**：界面上那一格是一个多选框，"这个人出场 / 不出场"点一下就落定——
//!    交上来的就是那一段完整的名单，不做"加一张 / 减一张"两套接口（两套接口就有两处
//!    幂等语义要对齐，而这里的变化理由只有一个：作者改了名单）；
//! 2. **关联认 id，不认名字**：卡改名不动关联（名字在 `entity_cards` 那一份里）；
//! 3. **软删的卡自动消失**：读的时候 JOIN `deleted_at IS NULL`——作者删掉一张人物卡，
//!    它不会留在每一章的人名里（真清掉时外键级联收尾）；
//! 4. **只有承载正文的节点有名单**：卷是分组行，没有"这一章有谁出场"这回事
//!    （与四格同一条口径，见 [`Store::has_fields`]）。

use std::collections::HashMap;

use rusqlite::params;
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 名单里的一行：设定卡的 id + 它现在的名字。
///
/// 名字是**读的那一刻**从卡上取的：改名之后下一次读就是新名，关联不需要跟着改。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CastMember {
    pub entity_id: i64,
    pub name: String,
}

impl Store {
    /// 这本书里**每个节点**的出场人物（没挂过人的节点不会出现在结果里）。一次拿全。
    ///
    /// 大纲表要一屏摆完这本书：一条章一次往返会把"这本书有多少章"变成 N 次查询。
    pub fn node_cast(&self, work_id: i64) -> Result<HashMap<i64, Vec<CastMember>>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.node_id, c.entity_id, e.name
               FROM node_cast c
               JOIN nodes n        ON n.id = c.node_id AND n.deleted_at IS NULL
               JOIN entity_cards e ON e.id = c.entity_id AND e.deleted_at IS NULL
              WHERE n.work_id = ?1
              ORDER BY e.name, c.entity_id",
        )?;
        let rows = stmt.query_map(params![work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                CastMember { entity_id: row.get(1)?, name: row.get(2)? },
            ))
        })?;
        let mut out: HashMap<i64, Vec<CastMember>> = HashMap::new();
        for row in rows {
            let (node_id, member) = row?;
            out.entry(node_id).or_default().push(member);
        }
        Ok(out)
    }

    /// 一段的出场人物（按卡的名字排）。
    pub fn node_cast_of(&self, node_id: i64) -> Result<Vec<CastMember>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.name
               FROM node_cast c
               JOIN entity_cards e ON e.id = c.entity_id AND e.deleted_at IS NULL
              WHERE c.node_id = ?1
              ORDER BY e.name, e.id",
        )?;
        let rows = stmt.query_map(params![node_id], |row| {
            Ok(CastMember { entity_id: row.get(0)?, name: row.get(1)? })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 换掉一段的出场人物（**整份覆盖**），返回库里真有的那一份。
    ///
    /// 先校验完再动一个字：名单里有一张别人的卡，整次操作当场拒——
    /// 半份名单写进去比不写难查得多（与整片粘贴同一条纪律）。
    pub fn set_node_cast(
        &mut self,
        node_id: i64,
        entity_ids: &[i64],
        trigger: &str,
    ) -> Result<Vec<CastMember>> {
        let work_id = self.node_work(node_id)?;
        if !self.has_fields(node_id)? {
            // 卷这种分组行没有"出场人物"这回事（界面根本不摆这一格，这是入库前的最后一关）
            return Err(Error::invalid_with(
                codes::CAST_NODE_NO_BODY,
                [("node_id", node_id.to_string())],
            ));
        }
        // 去重（同一个人点两下只算一次），并逐张核对：这本书的、还活着的卡
        let mut wanted: Vec<i64> = Vec::new();
        for id in entity_ids {
            if wanted.contains(id) {
                continue;
            }
            let card = self.entity_card(*id)?;
            if card.work_id != work_id {
                return Err(Error::invalid_with(
                    codes::CAST_ENTITY_FOREIGN,
                    [("entity_id", id.to_string()), ("work_id", work_id.to_string())],
                ));
            }
            wanted.push(*id);
        }

        let before: Vec<i64> = self
            .conn
            .prepare("SELECT entity_id FROM node_cast WHERE node_id = ?1 ORDER BY entity_id")?
            .query_map(params![node_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;

        let now = now_millis();
        let tx = self.conn.transaction()?;
        // 只动真变了的那几笔：没变的关联保留原样的 `created_at`（"什么时候挂上去的"不许被重写）
        let removed: Vec<i64> = before.iter().copied().filter(|id| !wanted.contains(id)).collect();
        let added: Vec<i64> = wanted.iter().copied().filter(|id| !before.contains(id)).collect();
        for id in &removed {
            tx.execute("DELETE FROM node_cast WHERE node_id = ?1 AND entity_id = ?2", params![node_id, id])?;
        }
        for id in &added {
            tx.execute(
                "INSERT INTO node_cast(node_id, entity_id, created_at) VALUES(?1, ?2, ?3)",
                params![node_id, id, now],
            )?;
        }
        if !added.is_empty() || !removed.is_empty() {
            Self::record_in(
                &self.device_id,
                &tx,
                "node_cast",
                node_id,
                "set",
                json!({
                    "added": added.len(),
                    "removed": removed.len(),
                    "total": wanted.len(),
                    "trigger": trigger,
                }),
            )?;
        }
        tx.commit()?;
        self.node_cast_of(node_id)
    }
}
