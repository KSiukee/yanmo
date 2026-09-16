//! 伏笔的读写：**埋下 → 收了 / 不写了**（表与状态机见 `model::foreshadow`）。
//!
//! 这里只管四件事：记一条、改状态（带合法性检查）、列、软删；
//! "埋了多久没收"的判定在 [`crate::outline`]（纯逻辑，数据由 [`super::outline_scan`] 喂）。
//!
//! 三条分寸：
//!
//! 1. **不收也能是结局**：`dropped`（不写了）是正经状态，不是失败——检测只报 `planted`；
//! 2. **迁移要留痕**：每次状态变化写 op-log（从哪个态到哪个态、收在哪一章）；
//! 3. **软删**：与别的卡一样，删只是打时间戳。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{work_alive, Store};
use crate::error::{codes, Error, Result};
use crate::model::{Foreshadow, ForeshadowState, NewForeshadow};
use crate::time::now_millis;

/// 读一条用的列（顺序与 [`read_raw`] 一一对应）。
const COLS: &str = "id, work_id, body, planted_node, collected_node, state, note, created_at, updated_at";

fn read_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Option<i64>, String, Option<i64>, Option<i64>, String, String, i64, i64)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

/// 把一行变成一条伏笔：**认不出的状态如实报错**（与问题卡同一条纪律）。
fn into_foreshadow(
    raw: (i64, Option<i64>, String, Option<i64>, Option<i64>, String, String, i64, i64),
) -> Result<Foreshadow> {
    Ok(Foreshadow {
        id: raw.0,
        // 与别的卡同一条口径：库里被手改成没有归属的按作品 0 算（如实显示，不假装有主）
        work_id: raw.1.unwrap_or(0),
        body: raw.2,
        planted_node: raw.3,
        collected_node: raw.4,
        state: ForeshadowState::parse(&raw.5)?,
        note: raw.6,
        created_at: raw.7,
        updated_at: raw.8,
    })
}

/// 「这条伏笔不存在」：只有一处说法。
fn missing(id: i64) -> Error {
    Error::invalid_with(codes::FORESHADOW_NOT_FOUND, [("foreshadow_id", id.to_string())])
}

/// 锚点校验：埋点 / 收点必须是**这本书里、还在**的节点。
///
/// 为什么在核心挡这一道：外键确实也会拦住脏锚点，但那样报出来的是
/// "FOREIGN KEY constraint failed"——**数据库约束的话不该让作者看见**，
/// 而我们本来就能说清"这一章不能当落点"（不存在 / 已删 / 别的书的）。
fn check_anchor(store: &Store, work_id: i64, node: Option<i64>) -> Result<()> {
    let Some(node_id) = node else { return Ok(()) };
    let owner: Option<i64> = store
        .conn()
        .query_row(
            "SELECT work_id FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
            params![node_id],
            |row| row.get(0),
        )
        .optional()?;
    if owner != Some(work_id) {
        return Err(Error::invalid_with(
            codes::FORESHADOW_ANCHOR_INVALID,
            [("node_id", node_id.to_string())],
        ));
    }
    Ok(())
}

impl Store {
    /// 记一条伏笔（状态从「埋着」开始）；正文不许空。
    pub fn create_foreshadow(&mut self, new: &NewForeshadow, trigger: &str) -> Result<i64> {
        let body = new.body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::FORESHADOW_BODY_EMPTY));
        }
        check_anchor(self, new.work_id, new.planted_node)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        if !work_alive(&tx, new.work_id)? {
            return Err(Error::invalid_with(
                codes::WORK_GONE,
                [("work_id", new.work_id.to_string())],
            ));
        }
        tx.execute(
            "INSERT INTO foreshadows(work_id, body, planted_node, collected_node, state, note,
                                     created_at, updated_at)
             VALUES(?1, ?2, ?3, NULL, ?4, ?5, ?6, ?6)",
            params![
                new.work_id,
                body,
                new.planted_node,
                ForeshadowState::Planted.as_str(),
                new.note.trim(),
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "foreshadows",
            id,
            "create",
            json!({
                "planted_node": new.planted_node,
                "to": ForeshadowState::Planted.as_str(),
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 改一条伏笔的正文 / 埋点 / 备注（**不改状态**——状态走 [`Store::move_foreshadow`]）。
    pub fn update_foreshadow(
        &mut self,
        id: i64,
        body: &str,
        planted_node: Option<i64>,
        note: &str,
        trigger: &str,
    ) -> Result<Foreshadow> {
        let body = body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::FORESHADOW_BODY_EMPTY));
        }
        let before = self.foreshadow(id)?;
        check_anchor(self, before.work_id, planted_node)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE foreshadows SET body = ?1, planted_node = ?2, note = ?3, updated_at = ?4
              WHERE id = ?5 AND deleted_at IS NULL",
            params![body, planted_node, note.trim(), now, id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "foreshadows",
            id,
            "update",
            json!({ "planted_node": planted_node, "was_node": before.planted_node,
                    "trigger": trigger }),
        )?;
        tx.commit()?;
        self.foreshadow(id)
    }

    /// 走一步状态：`collected` 要带上**收在哪一章**；回到「埋着」时把收点清掉。
    ///
    /// 非法边当场拒（`foreshadow.illegal_transition`）——界面上的按钮由
    /// [`ForeshadowState::next_states`] 摆，这里再挡一道，坏调用进不来。
    pub fn move_foreshadow(
        &mut self,
        id: i64,
        to: ForeshadowState,
        collected_node: Option<i64>,
        trigger: &str,
    ) -> Result<Foreshadow> {
        let before = self.foreshadow(id)?;
        check_anchor(self, before.work_id, collected_node)?;
        if !ForeshadowState::can_move(before.state, to) {
            return Err(Error::invalid_with(
                codes::FORESHADOW_ILLEGAL_TRANSITION,
                [
                    ("from", before.state.as_str().to_string()),
                    ("to", to.as_str().to_string()),
                ],
            ));
        }
        // 收了才有收点；"收在哪一章"是可以不记的（作者没填就不填）
        let collected = if to == ForeshadowState::Collected {
            collected_node.or(before.collected_node)
        } else {
            None
        };
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE foreshadows SET state = ?1, collected_node = ?2, updated_at = ?3
              WHERE id = ?4 AND deleted_at IS NULL",
            params![to.as_str(), collected, now, id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "foreshadows",
            id,
            "move",
            json!({
                "from": before.state.as_str(),
                "to": to.as_str(),
                "collected_node": collected,
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        self.foreshadow(id)
    }

    /// 取一条（软删的不算）。
    pub fn foreshadow(&self, id: i64) -> Result<Foreshadow> {
        let sql = format!("SELECT {COLS} FROM foreshadows WHERE id = ?1 AND deleted_at IS NULL");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_raw)
            .optional()?
            .ok_or_else(|| missing(id))?;
        into_foreshadow(raw)
    }

    /// 一本书里的伏笔，可按状态筛（`None` = 全都要）；**埋着的排前面，其余按埋点**。
    pub fn foreshadows(
        &self,
        work_id: i64,
        state: Option<ForeshadowState>,
    ) -> Result<Vec<Foreshadow>> {
        let sql = format!(
            "SELECT {COLS} FROM foreshadows
              WHERE work_id = ?1 AND deleted_at IS NULL
                AND (?2 IS NULL OR state = ?2)
              ORDER BY CASE state WHEN 'planted' THEN 0 ELSE 1 END, planted_node, id"
        );
        let wanted = state.map(ForeshadowState::as_str);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id, wanted], read_raw)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_foreshadow(row?)?);
        }
        Ok(out)
    }

    /// 删一条：**只打时间戳**，返回它删掉的那一条（调用方要靠 `work_id` 回一屏最新的）。
    pub fn delete_foreshadow(&mut self, id: i64, trigger: &str) -> Result<Foreshadow> {
        let gone = self.foreshadow(id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE foreshadows SET deleted_at = ?1, updated_at = ?1
              WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "foreshadows",
            id,
            "delete",
            json!({ "state": gone.state.as_str(), "trigger": trigger }),
        )?;
        tx.commit()?;
        Ok(gone)
    }
}
