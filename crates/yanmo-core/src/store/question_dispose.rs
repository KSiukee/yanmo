//! 处置的两件外围事：**冷却库**（舍弃的卡去哪了）与**按来源静音**。
//!
//! # 冷却库：舍弃不等于删除
//!
//! 舍弃的卡进冷却库——**可捞回**，同时作为选题的负样本（"他舍弃过这类"，见
//! [`super::question_weights`]）。为什么不真删：作者此刻烦躁点了舍弃，两周后可能又想要；
//! 而那条"他不爱答这类"的教训，也只有留着记录才学得到。
//!
//! **两档冷却不另造机制**（产品口径的诚实映射）：
//! - **短冷却**＝延后（带条件，几章／几天后以低权回来，见 [`super::question_defer_write`]）；
//! - **长冷却**＝舍弃（几乎不再出现，只躺在冷却库里等作者自己捞）。
//!
//! # 按来源静音：模块太吵时一键让它闭嘴
//!
//! 静音**按类**（模板）与**按卡**都落在状态机与模板权重上；这里补第三种：**按来源**
//! （核心自带写 `core`，模块提交的写模块名）。某个模块太吵时，作者该能只让它闭嘴，
//! 而不是把整个叩问关掉。
//!
//! 存在 `settings` 的键值里（与外观偏好同一处，**不新建表、不动迁移**），
//! 内容是来源名的 JSON 数组；读不出来就当"一个都没静音"（坏记录当没设过）。

use rusqlite::params;
use serde::Serialize;

use super::Store;
use crate::error::Result;
use crate::model::{QuestionCard, QuestionState};
use crate::time::now_millis;

/// 静音来源在 `settings` 里的键。
const MUTED_SOURCES_KEY: &str = "question.muted_sources";

/// 冷却库里的一行：卡 + 什么时候舍弃的（按这个倒序，最近舍弃的在前）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CooledCard {
    pub card_id: i64,
    pub template_key: String,
    pub body: String,
    pub source: String,
    /// 舍弃的时刻（`updated_at` 在那次迁移里被盖上）
    pub cooled_at: i64,
}

fn muted_sources_from(conn: &rusqlite::Connection) -> Result<Vec<String>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![MUTED_SOURCES_KEY],
            |r| r.get(0),
        )
        .ok();
    Ok(raw
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default())
}

impl Store {
    /// 冷却库：这本书里**舍弃过**的卡，最近舍弃的在前。
    pub fn cooled_questions(&self, work_id: i64) -> Result<Vec<CooledCard>> {
        let mut cards = self.question_cards(work_id, Some(QuestionState::Discarded))?;
        // 按舍弃时刻倒序（`question_cards` 给的是创建顺序，这里要的是"最近放下的"）
        cards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
        Ok(cards
            .into_iter()
            .map(|card: QuestionCard| CooledCard {
                card_id: card.id,
                template_key: card.template_key,
                body: card.body,
                source: card.source,
                cooled_at: card.updated_at,
            })
            .collect())
    }

    /// 已经静音的来源（面板上要显示、还要能解除）。
    pub fn muted_sources(&self) -> Result<Vec<String>> {
        muted_sources_from(&self.conn)
    }

    /// 把一个来源整体静音：它提的问题以后不再进候选池（**不动已经存在的卡**——
    /// 那些卡的处置是作者自己的决定，静音只影响"要不要再摆到他面前"）。
    pub fn mute_source(&mut self, source: &str) -> Result<()> {
        let source = source.trim();
        if source.is_empty() {
            return Ok(());
        }
        let mut sources = self.muted_sources()?;
        if !sources.iter().any(|item| item == source) {
            sources.push(source.to_string());
            self.write_muted_sources(&sources)?;
        }
        Ok(())
    }

    /// 解除一个来源的静音。
    pub fn unmute_source(&mut self, source: &str) -> Result<()> {
        let mut sources = self.muted_sources()?;
        let before = sources.len();
        sources.retain(|item| item != source);
        if sources.len() != before {
            self.write_muted_sources(&sources)?;
        }
        Ok(())
    }

    fn write_muted_sources(&mut self, sources: &[String]) -> Result<()> {
        let key = MUTED_SOURCES_KEY;
        if sources.is_empty() {
            // 一个都不静音时把记录删掉，别在库里留一个空数组（"没设过"与"设成空"是一回事）
            self.conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            return Ok(());
        }
        let json = serde_json::to_string(sources).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![key, json, now_millis()],
        )?;
        Ok(())
    }
}
