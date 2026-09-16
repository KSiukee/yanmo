//! 场景卡四格的读写：**视角 / 目标 / 冲突 / 结果**。
//!
//! 四格住在卫星表 `scene_cards`（`node_id` 主键，理由见 `db::migrations_v11`）：
//! **没有这一行 = 这张卡还没填过**，读出来就是四个空串——所以"缺项"这件事
//! 天然是"值为空"，不必再造一个"填过没有"的布尔。
//!
//! 两条分寸：
//!
//! 1. **只写传进来的那份**：四格是一整张小表单（一次全给），所以整行覆盖；
//! 2. **值原样存**（修剪首尾空白）：不替作者判断哪句话算不算"目标"——
//!    冲突检测那一层只问"填没填"，不问"写得对不对"。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{NodeKind, SceneFields};
use crate::time::now_millis;

impl Store {
    /// 这个节点是不是场景卡（**编辑器靠它决定露不露那四格**）。
    ///
    /// 单独给一个谓词，而不是让调用方去 `match` "不是场景卡"那条错误码：
    /// **用异常当控制流**会把"读坏了"与"本来就没有"混成一条路（前者该报，后者是常态）。
    pub fn is_scene(&self, node_id: i64) -> Result<bool> {
        let kind: Option<String> = self
            .conn
            .query_row(
                "SELECT node_kind FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(kind.as_deref() == Some(NodeKind::Scene.as_str()))
    }

    /// 取一张场景卡的四格。**不是场景卡就报错**（目录树上任何别的节点都没有这四格）。
    pub fn scene_fields(&self, node_id: i64) -> Result<SceneFields> {
        let kind: Option<String> = self
            .conn
            .query_row(
                "SELECT node_kind FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |row| row.get(0),
            )
            .optional()?;
        let kind = kind.ok_or_else(|| {
            Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
        })?;
        if kind != NodeKind::Scene.as_str() {
            return Err(Error::invalid_with(
                codes::NODE_NOT_SCENE,
                [("node_id", node_id.to_string())],
            ));
        }
        let row: Option<(String, String, String, String)> = self
            .conn
            .query_row(
                "SELECT pov, goal, conflict, outcome FROM scene_cards WHERE node_id = ?1",
                params![node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        // 没有这一行 = 四格都还没填
        Ok(match row {
            Some((pov, goal, conflict, outcome)) => SceneFields {
                node_id,
                pov,
                goal,
                conflict,
                outcome,
            },
            None => SceneFields::empty(node_id),
        })
    }

    /// 存一张场景卡的四格（整行覆盖），返回**库里真有的那一份**。
    pub fn save_scene_fields(
        &mut self,
        fields: &SceneFields,
        trigger: &str,
    ) -> Result<SceneFields> {
        // 先确认它是场景卡（顺带把"节点不在"与"不是场景卡"分开报）
        let before = self.scene_fields(fields.node_id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO scene_cards(node_id, pov, goal, conflict, outcome, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
                pov = excluded.pov, goal = excluded.goal, conflict = excluded.conflict,
                outcome = excluded.outcome, updated_at = excluded.updated_at",
            params![
                fields.node_id,
                fields.pov.trim(),
                fields.goal.trim(),
                fields.conflict.trim(),
                fields.outcome.trim(),
                now
            ],
        )?;
        // 留痕记**哪几格从空变有**（不记全文：正文那类东西不往 op-log 里塞）
        Self::record_in(
            &self.device_id,
            &tx,
            "scene_cards",
            fields.node_id,
            "write",
            json!({
                "filled_pov": before.pov.trim().is_empty() && !fields.pov.trim().is_empty(),
                "filled_goal": before.goal.trim().is_empty() && !fields.goal.trim().is_empty(),
                "filled_conflict":
                    before.conflict.trim().is_empty() && !fields.conflict.trim().is_empty(),
                "filled_outcome":
                    before.outcome.trim().is_empty() && !fields.outcome.trim().is_empty(),
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        self.scene_fields(fields.node_id)
    }

    /// 一本书里**所有场景卡**（按树里的顺序），带上它们的四格——冲突检测要用。
    ///
    /// 一次 JOIN 拿全：一条条去问会把"这本书有多少场景卡"变成 N 次往返。
    /// 返回 `(node_id, 名字, 四格)`。
    pub fn scene_cards_of_work(&self, work_id: i64) -> Result<Vec<(i64, String, SceneFields)>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.title,
                    COALESCE(s.pov, ''), COALESCE(s.goal, ''),
                    COALESCE(s.conflict, ''), COALESCE(s.outcome, '')
               FROM nodes n
               LEFT JOIN scene_cards s ON s.node_id = n.id
              WHERE n.work_id = ?1 AND n.node_kind = ?2 AND n.deleted_at IS NULL
              ORDER BY n.parent_id, n.sort_order, n.id",
        )?;
        let rows = stmt.query_map(params![work_id, NodeKind::Scene.as_str()], |row| {
            let node_id: i64 = row.get(0)?;
            let title: String = row.get(1)?;
            Ok((
                node_id,
                title,
                SceneFields {
                    node_id,
                    pov: row.get(2)?,
                    goal: row.get(3)?,
                    conflict: row.get(4)?,
                    outcome: row.get(5)?,
                },
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
