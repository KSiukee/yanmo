//! 外观与写作行为的偏好：**全局打底 + 每书可选覆盖**。
//!
//! 三条规矩：
//! 1. **只存改过的项**：库里存的是"空档"（`None` = 没改过），读的时候才落默认值——
//!    将来默认值改了，没动过这一项的旧库也跟着变，不必写迁移。
//! 2. **书的覆盖盖在全局上**：每本书的键只在作者真的"单独设过"时才存在；没设就继承全局。
//! 3. **读不出来的记录当没设过**：宁可回默认，也不要让一个坏 JSON 把界面卡住。
//!
//! 与正文无关：这些偏好**不进导出**，也不改动数据的任何字节。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::Result;
use crate::time::now_millis;

/// 作者改过的项（`None` = 没改过，用默认）。
///
/// 加新项就往这里加一个 `Option<...>` 字段：老库读得进来（缺的字段算没改过），
/// 新库被老版本读到也只是多一个不认识的字段——**不必写迁移**。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    /// 打开"最新那一章"时，跳到段末并聚焦输入光标
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_to_end_on_latest: Option<bool>,
}

impl Appearance {
    /// 一项都没改过——那就没必要在库里留这个键。
    fn is_empty(&self) -> bool {
        self.jump_to_end_on_latest.is_none()
    }

    /// 把 `over`（书的覆盖）盖在 `self`（全局）上：**只覆盖它真设过的项**。
    fn overridden_by(&self, over: &Appearance) -> Appearance {
        Appearance {
            jump_to_end_on_latest: over.jump_to_end_on_latest.or(self.jump_to_end_on_latest),
        }
    }
}

/// 读出来给人用的那一份：每一项都已经落到具体值（不再有空档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResolvedAppearance {
    pub jump_to_end_on_latest: bool,
}

impl Default for ResolvedAppearance {
    /// 默认值只有这一处——界面与核心都不许各写一份。
    fn default() -> Self {
        Self { jump_to_end_on_latest: true }
    }
}

impl Store {
    /// 读偏好：**书的覆盖 → 全局 → 默认**。
    ///
    /// `work_id` 给 `Some` 会先看这本书有没有单独设过；给 `None` 只看全局。
    pub fn appearance(&self, work_id: Option<i64>) -> Result<ResolvedAppearance> {
        let global = self.read_appearance(None)?;
        let merged = match work_id {
            Some(id) => global.overridden_by(&self.read_appearance(Some(id))?),
            None => global,
        };
        let defaults = ResolvedAppearance::default();
        Ok(ResolvedAppearance {
            jump_to_end_on_latest: merged
                .jump_to_end_on_latest
                .unwrap_or(defaults.jump_to_end_on_latest),
        })
    }

    /// 写偏好（**稀疏合并**）：只覆盖传进来的项，没传的保持原样。
    ///
    /// `work_id` 给 `Some` 就是"这本书单独设"，给 `None` 就是全局。
    pub fn set_appearance(&mut self, work_id: Option<i64>, patch: &Appearance) -> Result<()> {
        let mut stored = self.read_appearance(work_id)?;
        if let Some(value) = patch.jump_to_end_on_latest {
            stored.jump_to_end_on_latest = Some(value);
        }
        self.write_appearance(work_id, &stored)?;
        self.record(
            "settings",
            work_id.unwrap_or(0),
            "set_appearance",
            serde_json::json!({ "jump_to_end_on_latest": patch.jump_to_end_on_latest }),
        )
    }

    /// 清掉一份偏好，**回到默认**（书的覆盖则是"回到继承全局"）。
    ///
    /// 与"传一个空 patch"不同：空 patch 是"这项不改"，这里是真的把记录抹掉。
    pub fn reset_appearance(&mut self, work_id: Option<i64>) -> Result<()> {
        self.write_appearance(work_id, &Appearance::default())?;
        self.record(
            "settings",
            work_id.unwrap_or(0),
            "reset_appearance",
            serde_json::json!({}),
        )
    }

    /// 读一份（全局或某本书的覆盖）；读不出来当没设过。
    fn read_appearance(&self, work_id: Option<i64>) -> Result<Appearance> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![appearance_key(work_id)],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    /// 写一份；一项都没改过就把键删掉（不留空记录）。
    fn write_appearance(&self, work_id: Option<i64>, value: &Appearance) -> Result<()> {
        let key = appearance_key(work_id);
        if value.is_empty() {
            self.conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            return Ok(());
        }
        let json = serde_json::to_string(value).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![key, json, now_millis()],
        )?;
        Ok(())
    }
}

/// 偏好在 `settings` 里的键：全局一份，每本书可另存一份覆盖。
fn appearance_key(work_id: Option<i64>) -> String {
    match work_id {
        Some(id) => format!("work.{id}.appearance"),
        None => "appearance".to_string(),
    }
}
