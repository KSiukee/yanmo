//! 延后与重出（**写侧**）：把条件落下来、把到条件的卡放回候选池。
//!
//! 另一半（延后记录怎么读、怎么数）在 [`super::question_defer`]——两边分开，
//! 因为"作者点延后"与"界面显示它在等什么"的变化理由不一样。
//!
//! # 两条分寸
//!
//! 1. **状态与条件同一个事务**：延后要同时写条件与改状态，重出要同时标掉条件与改状态——
//!    分两次写就会留下"卡改了态、条件没写上"的半截状态（那种最难查）。
//! 2. **重出是显式动作**：`requeue_due_questions` 由调用方在合适的时机（开新章 / 保存 / 定时）
//!    喊一声。选题本身仍然**只读**——翻看问题不该顺手改库。
//!
//! # 判据
//!
//! - 时间：到点；
//! - 写到：锚点**子树里出现正文**（"这一章写完了"与"进到某一卷了"是同一条判据，
//!   差别只在锚点是章还是卷）；
//! - 只有作者：永不自动重出；
//! - **锚点已经不在书里了 → 算翻篇**（条件当作满足）：让作者删掉的那件事永远堵着一条问题，
//!   比"多问一次"更糟——而且堵着的那个才是真的死路。
//! - 条件字段缺了（库被人手改坏）也按**翻篇**处理，同上：宁可多问一次，不留死路。

use std::collections::HashMap;

use rusqlite::params;

use super::card_move::move_card_in;
use super::defer_condition::satisfied;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{QuestionCard, QuestionState};
use crate::question::{chapter_anchor, DeferCondition, DeferKind, DeferPreset};
use crate::time::now_millis;

impl Store {
    /// 延后一张卡：**带上条件**，并把卡迁到「延后」态——两件事同一个事务。
    ///
    /// 时间条件不许落在过去：那不是延后，是"点了延后它下一眼又冒出来"。
    /// 卡不在「已问」态时会被状态机拒掉（延后只有这一条来路）。
    pub fn defer_question_card(
        &mut self,
        card_id: i64,
        condition: DeferCondition,
        note: &str,
        trigger: &str,
    ) -> Result<i64> {
        let card = self.question_card(card_id)?;
        let now = now_millis();
        if condition.kind() == DeferKind::Time {
            let due_at = condition.due_at_ms().unwrap_or(i64::MIN);
            if due_at <= now {
                return Err(Error::invalid_with(
                    codes::QUESTION_DEFER_TIME_NOT_FUTURE,
                    [("due_at", due_at.to_string())],
                ));
            }
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO question_deferrals(card_id, kind, due_at_ms, anchor_node, note,
                                            created_at, resolved_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                card_id,
                condition.kind().as_str(),
                condition.due_at_ms(),
                condition.anchor_node(),
                note.trim(),
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        move_card_in(&tx, &self.device_id, &card, QuestionState::Deferred, trigger)?;
        tx.commit()?;
        Ok(id)
    }

    /// 按界面上那一档预置来延后（**作者点的是"什么时候再问我"**）。
    ///
    /// 「写完这一章再问」要一个章节锚点，这里直接从卡上找（候选生成时把 `chapter:<id>`
    /// 写进了卡的关联里）——作者不必再被问一遍"哪一章"。
    pub fn defer_question_card_by_preset(
        &mut self,
        card_id: i64,
        preset: DeferPreset,
        note: &str,
        trigger: &str,
    ) -> Result<i64> {
        let card = self.question_card(card_id)?;
        let anchor = chapter_anchor(&card.linked);
        let condition = preset.condition(now_millis(), anchor).ok_or_else(|| {
            Error::invalid_with(
                codes::QUESTION_DEFER_NEEDS_ANCHOR,
                [("card_id", card_id.to_string())],
            )
        })?;
        self.defer_question_card(card_id, condition, note, trigger)
    }

    /// 把**条件已经满足**的延后放回候选池：回「待问」+ 标掉那条记录（同一事务）。
    ///
    /// 返回重出的卡 id（按延后先后）。顺手把"卡已经不在延后态了"的旧记录标掉——
    /// 作者手动捞回之后那条记录不该一直挂着。
    pub fn requeue_due_questions(
        &mut self,
        work_id: i64,
        now_ms: i64,
        trigger: &str,
    ) -> Result<Vec<i64>> {
        // 读侧先做完（判条件要查库），写侧只开一个事务
        let open = self.open_deferrals(work_id)?;
        let cards: HashMap<i64, QuestionCard> =
            self.question_cards(work_id, None)?.into_iter().map(|c| (c.id, c)).collect();
        let mut due = Vec::new();
        let mut stale = Vec::new();
        for deferral in &open {
            match cards.get(&deferral.card_id) {
                Some(card) if card.state == QuestionState::Deferred => {
                    if satisfied(self.conn(), deferral, now_ms)? {
                        due.push((deferral.id, deferral.card_id, deferral.kind));
                    }
                }
                // 卡被删了，或已经不在延后态（作者手动捞回过）：记录收掉
                _ => stale.push(deferral.id),
            }
        }
        if due.is_empty() && stale.is_empty() {
            return Ok(Vec::new());
        }

        let now = now_millis();
        let tx = self.conn.transaction()?;
        for id in stale {
            tx.execute(
                "UPDATE question_deferrals SET resolved_at = ?1 WHERE id = ?2 AND resolved_at IS NULL",
                params![now, id],
            )?;
        }
        let mut requeued = Vec::new();
        for (deferral_id, card_id, kind) in due {
            let card = cards.get(&card_id).expect("checked just above");
            // 留痕里的 trigger 带上条件类型：事后一眼看得出是"哪条条件到了"
            move_card_in(
                &tx,
                &self.device_id,
                card,
                QuestionState::Pending,
                &format!("{trigger}:{}", kind.as_str()),
            )?;
            tx.execute(
                "UPDATE question_deferrals SET resolved_at = ?1 WHERE id = ?2 AND resolved_at IS NULL",
                params![now, deferral_id],
            )?;
            requeued.push(card_id);
        }
        tx.commit()?;
        Ok(requeued)
    }

}
