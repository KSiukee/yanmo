//! **四格**（视角 / 目标 / 冲突 / 结果）的读写。
//!
//! 四格住在卫星表 `scene_cards`（`node_id` 主键，理由见 `db::migrations_v11`）：
//! **没有这一行 = 还没填过**，读出来就是四个空串——所以"缺项"这件事
//! 天然是"值为空"，不必再造一个"填过没有"的布尔。
//!
//! # 归谁有（2026-09-16 放宽）
//!
//! 原来是"场景卡才有"；**现在凡承载正文的节点都有**（章 / 节 / 单篇 / 场景卡）。
//! 理由：中文网文作者的习惯是**一章一行**（Excel 细纲表那一派），
//! 非要先建一张场景卡才填得了，等于给日更加了一道工序；
//! 场景卡仍是"想把一场戏单独拆出来写"时的那个细粒度。
//! 表名与 `SceneFields` 这个名字**保持不变**（改名要动 schema，而迁移只增不改）——
//! 这里把它记清楚，别让下一个读者被名字误导。
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
    /// 这个节点能不能有四格（**编辑器与大纲表靠它决定露不露那几列**）。
    ///
    /// 单独给一个谓词，而不是让调用方去 `match` 那条错误码：
    /// **用异常当控制流**会把"读坏了"与"本来就没有"混成一条路（前者该报，后者是常态）。
    pub fn has_fields(&self, node_id: i64) -> Result<bool> {
        let kind: Option<String> = self
            .conn
            .query_row(
                "SELECT node_kind FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |row| row.get(0),
            )
            .optional()?;
        // 认不出的类型当"没有四格"（坏数据不该让整张表读不出来）
        Ok(kind.as_deref().and_then(|code| NodeKind::parse(code).ok()).is_some_and(|kind| kind.holds_body()))
    }

    /// 取一个节点的四格。**不承载正文的节点（卷）没有这四格，如实拒**。
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
        let holds_body = NodeKind::parse(&kind).map(|kind| kind.holds_body()).unwrap_or(false);
        if !holds_body {
            return Err(Error::invalid_with(
                codes::NODE_NO_FIELDS,
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

    /// 存一个节点的四格（整行覆盖），返回**库里真有的那一份**。
    pub fn save_scene_fields(
        &mut self,
        fields: &SceneFields,
        trigger: &str,
    ) -> Result<SceneFields> {
        // 先确认它承载正文（顺带把"节点不在"与"卷这种没有四格的"分开报）
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

    /// 这本书里**填过四格的节点**（章 / 节 / 单篇 / 场景卡），带上四格——冲突检测要用。
    ///
    /// 只取**有那一行**的：没有这一行＝一个字都没填过，"缺项"在体检里不念
    /// （一个都没填是"还没打算填"，不是"填漏了"；那一类要看去大纲表里的筛选项）。
    /// 一次 JOIN 拿全：一条条去问会把"这本书有多少节点"变成 N 次往返。
    pub fn nodes_with_fields(&self, work_id: i64) -> Result<Vec<(i64, String, SceneFields)>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.title,
                    COALESCE(s.pov, ''), COALESCE(s.goal, ''),
                    COALESCE(s.conflict, ''), COALESCE(s.outcome, '')
               FROM nodes n
               JOIN scene_cards s ON s.node_id = n.id
              WHERE n.work_id = ?1 AND n.deleted_at IS NULL
              ORDER BY n.parent_id, n.sort_order, n.id",
        )?;
        let rows = stmt.query_map(params![work_id], |row| {
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
