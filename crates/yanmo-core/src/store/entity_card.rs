//! 设定卡的读写：**人物 / 设定**——名字、别称、属性键值、备注。
//!
//! 表与理由见 [`crate::model::entity_card`] 与 `db::migrations_v11`：它是**实体**，
//! 与碎片（素材）分家住。这里只管四件事：建、改（整卡覆盖）、列、软删；
//! 冲突检测在 [`crate::outline`]（那一层是纯逻辑，数据由 [`super::outline_scan`] 喂）。
//!
//! 两条分寸：
//!
//! 1. **名字是身份**：不许空（修剪后）；别称与属性里的空项**入口就丢掉**——
//!    那是手滑，不是作者的设定（留着只会在检测时冒出一堆"空名字撞车"的假问题）；
//! 2. **软删**：与作品、章节、碎片同一条纪律（删只是打时间戳，留痕在 op-log）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{work_alive, Store};
use crate::error::{codes, Error, Result};
use crate::model::{
    into_entity_card, Attribute, EntityCard, EntityKind, NewEntityCard, RawEntityCard,
};
use crate::time::now_millis;

/// 读一卡用的列（顺序与 [`read_raw`] 一一对应）。
const COLS: &str = "id, work_id, card_kind, name, aliases, attributes, note, created_at, updated_at";

fn read_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawEntityCard> {
    Ok(RawEntityCard {
        id: row.get(0)?,
        work_id: row.get(1)?,
        kind: row.get(2)?,
        name: row.get(3)?,
        aliases: row.get(4)?,
        attributes: row.get(5)?,
        note: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// 「这张卡不存在」：只有一处说法。
fn missing(id: i64) -> Error {
    Error::invalid_with(codes::ENTITY_NOT_FOUND, [("entity_id", id.to_string())])
}

/// 把作者给的那一份**修剪成能入库的形状**：名字要去空白；别称与属性丢掉空项。
///
/// 返回 `(名字, 别称, 属性)`；名字空着就报错（名字是身份，不能没有）。
fn clean(new: &NewEntityCard) -> Result<(String, Vec<String>, Vec<Attribute>)> {
    let name = new.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::invalid(codes::ENTITY_NAME_EMPTY));
    }
    let aliases: Vec<String> = new
        .aliases
        .iter()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .collect();
    let attributes: Vec<Attribute> = new
        .attributes
        .iter()
        .map(|attr| Attribute {
            key: attr.key.trim().to_string(),
            value: attr.value.trim().to_string(),
        })
        // 键与值都空的整条丢掉；只有键没值的那条**留着**——"还没填"是作者的状态，
        // 不是手滑（检测那一层会把它当"值空着"，由界面说清楚）
        .filter(|attr| !attr.key.is_empty() || !attr.value.is_empty())
        .collect();
    Ok((name, aliases, attributes))
}

impl Store {
    /// 新建一张设定卡（写卡 + 留痕，同一事务）。
    pub fn create_entity_card(&mut self, new: &NewEntityCard, trigger: &str) -> Result<i64> {
        let (name, aliases, attributes) = clean(new)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        if !work_alive(&tx, new.work_id)? {
            return Err(Error::invalid_with(
                codes::WORK_GONE,
                [("work_id", new.work_id.to_string())],
            ));
        }
        tx.execute(
            "INSERT INTO entity_cards(work_id, card_kind, name, aliases, attributes, note,
                                      created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                new.work_id,
                new.kind.as_str(),
                name,
                serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&attributes).unwrap_or_else(|_| "[]".to_string()),
                new.note.trim(),
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "entity_cards",
            id,
            "create",
            json!({ "kind": new.kind.as_str(), "name": name, "trigger": trigger }),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 改一张卡（**整卡覆盖**：界面上一个表单全摆着，不做稀疏合并）。
    pub fn update_entity_card(
        &mut self,
        id: i64,
        new: &NewEntityCard,
        trigger: &str,
    ) -> Result<()> {
        let (name, aliases, attributes) = clean(new)?;
        // 先取一次：既确认它在，也拿到它属于哪本书
        let before = self.entity_card(id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE entity_cards
                SET card_kind = ?1, name = ?2, aliases = ?3, attributes = ?4, note = ?5,
                    updated_at = ?6
              WHERE id = ?7 AND deleted_at IS NULL",
            params![
                new.kind.as_str(),
                name,
                serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&attributes).unwrap_or_else(|_| "[]".to_string()),
                new.note.trim(),
                now,
                id
            ],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "entity_cards",
            id,
            "update",
            json!({
                "kind": new.kind.as_str(),
                "name": name,
                "was_name": before.name,
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 取一张卡（软删的不算）。
    pub fn entity_card(&self, id: i64) -> Result<EntityCard> {
        let sql = format!("SELECT {COLS} FROM entity_cards WHERE id = ?1 AND deleted_at IS NULL");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_raw)
            .optional()?
            .ok_or_else(|| missing(id))?;
        into_entity_card(raw)
    }

    /// 一本书里的设定卡，可按类型筛（`None` = 全都要）；**按名字排**（人物那一栏要好找）。
    pub fn entity_cards(
        &self,
        work_id: i64,
        kind: Option<EntityKind>,
    ) -> Result<Vec<EntityCard>> {
        let sql = format!(
            "SELECT {COLS} FROM entity_cards
              WHERE work_id = ?1 AND deleted_at IS NULL
                AND (?2 IS NULL OR card_kind = ?2)
              ORDER BY name, id"
        );
        let wanted = kind.map(EntityKind::as_str);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id, wanted], read_raw)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_entity_card(row?)?);
        }
        Ok(out)
    }

    /// 删一张卡：**只打时间戳**，返回它删掉的那一张（调用方要靠 `work_id` 回一屏最新的）。
    pub fn delete_entity_card(&mut self, id: i64, trigger: &str) -> Result<EntityCard> {
        let card = self.entity_card(id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE entity_cards SET deleted_at = ?1, updated_at = ?1
              WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "entity_cards",
            id,
            "delete",
            json!({ "name": card.name, "trigger": trigger }),
        )?;
        tx.commit()?;
        Ok(card)
    }
}
