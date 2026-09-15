//! 记灵感：问题卡是**灵感发生器**——作者被一个问题勾起新念头时，就地落一张灵感卡。
//!
//! 三条分寸（设计口径）：
//!
//! 1. **正交**：记灵感**不动问题的状态**——一个问题完全可以既被延后、又派生了灵感卡，
//!    不是二选一；
//! 2. **不打断**：这里只写一张碎片，没有别的副作用（界面那半边的"轻量浮层、记完回到原问题、
//!    状态不变"照这个语义做）；
//! 3. **溯源**：灵感卡带着 `derived_from` 指回问题卡——"这条灵感是哪个问题勾出来的"永远查得到。
//!
//! 灵感卡住在**碎片统一表**里（`frag_kind = 'idea'`），所以它天生获得引力的全套机制
//! （新鲜度、被用过的次数、关联）；本文件只管"记下来"与"查回去"。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{InputSource, QuestionCard};
use crate::time::now_millis;

/// 灵感卡在碎片统一表里的种类码（写进 `fragments.frag_kind`；**别改**）。
const KIND_IDEA: &str = "idea";

/// 一张灵感卡（作者被问题勾起的那一句）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inspiration {
    pub id: i64,
    pub work_id: i64,
    pub body: String,
    /// 怎么记下的：`typed` / `voice` / `mixed`（答案输入方式记在同一列上）
    pub source: String,
    /// 溯源：哪个问题卡把它勾出来的
    pub derived_from: Option<i64>,
    pub created_at: i64,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Option<i64>, String, String, Option<i64>, i64)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn into_idea(raw: (i64, Option<i64>, String, String, Option<i64>, i64)) -> Inspiration {
    Inspiration {
        id: raw.0,
        // 与问题卡同一条口径：库里被手改成没有归属的按作品 0 算（如实显示，不假装有主）
        work_id: raw.1.unwrap_or(0),
        body: raw.2,
        source: raw.3,
        derived_from: raw.4,
        created_at: raw.5,
    }
}

impl Store {
    /// 记一条灵感：**从一张问题卡勾出来的**（写碎片 + 留痕，同一事务）。
    ///
    /// 问题的状态一个字节都不动——"作者被这个问题勾到了"不是"作者答了它"。
    /// 灵感卡沿用问题卡的关联锚点（之后按同一章/卷就能把它找回来）。
    pub fn record_question_inspiration(
        &mut self,
        card_id: i64,
        body: &str,
        source: &str,
        trigger: &str,
    ) -> Result<i64> {
        let body = body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::IDEA_BODY_EMPTY));
        }
        // 输入方式与答案走同一个闭集：认不出来的当场拒绝（留痕里的一列不能写歪）
        let source = InputSource::parse(source)?.as_str();
        let card: QuestionCard = self.question_card(card_id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO fragments(work_id, frag_kind, body, source, status, used_count,
                                   last_asked_at, linked, derived_from, auto_derived, template_key,
                                   created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 0, NULL, ?6, ?7, 0, '', ?8, ?8)",
            params![
                card.work_id,
                KIND_IDEA,
                body,
                source,
                // 碎片的 status 语义归创作流那条线；这里给一个中性值，别让问题态混进来
                "pending",
                card.linked,
                card_id,
                now
            ],
        )?;
        let idea_id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            idea_id,
            "inspire",
            serde_json::json!({
                "from": card.state.as_str(),
                "to": card.state.as_str(), // 状态不变：记灵感与处置正交
                "trigger": trigger,
                "source": source,
                "derived_from": card_id,
            }),
        )?;
        tx.commit()?;
        Ok(idea_id)
    }

    /// 取一张灵感卡。
    pub fn inspiration(&self, idea_id: i64) -> Result<Inspiration> {
        let raw = self
            .conn
            .query_row(
                "SELECT id, work_id, body, source, derived_from, created_at FROM fragments
                  WHERE id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL",
                params![idea_id, KIND_IDEA],
                read_row,
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::IDEA_NOT_FOUND, [("idea_id", idea_id.to_string())])
            })?;
        Ok(into_idea(raw))
    }

    /// 一个问题卡勾出过哪些灵感（按记下的先后）。
    pub fn inspirations_of_question(&self, card_id: i64) -> Result<Vec<Inspiration>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, work_id, body, source, derived_from, created_at FROM fragments
              WHERE derived_from = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
              ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![card_id, KIND_IDEA], read_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_idea(row?));
        }
        Ok(out)
    }
}
