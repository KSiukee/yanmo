//! 主动问一句（推）：**过了门槛才开口，开口就算"问过"**。
//!
//! 门槛（判据 + 配额 + 冷却）是纯逻辑，在 [`crate::question::push`]；这里做四件事：
//!
//! 1. **读一个事实**："当前这一章有没有正文"——空章判据要它（那句话在空章里不成立）；
//!    只有存储层读得出来，机制层照样不碰 SQL；
//! 2. **记账**：今天推了几次、上次什么时候——存在 `settings` 的键值里（不新建表、不动迁移），
//!    按作者本地那一天算；
//! 3. **挑一张**：与面板同一套选题（作者在哪一章就优先问那一章的），取最靠前的那张；
//! 4. **算问过**：把卡从「待问」迁到「已问」（`trigger = push:<时机>`）——**显示即消耗新颖度**，
//!    作者就算没答，这一条也已经被问出来了（那是实话：他看见过）。
//!
//! **卡与账同一个事务**：要么"问出去了并且记了账"，要么两样都不发生。
//! 只记一半的话，配额会算少（软件比作者以为的更爱开口），那正是这套护栏要防的事。

use rusqlite::{params, Connection};
use serde::Serialize;

use super::card_move::move_card_in;
use super::Store;
use crate::error::Result;
use crate::model::QuestionState;
use crate::question::{decide, PushDecision, PushQuota, PushReason, PushState};
use crate::time::now_millis;

/// 推的账存在 `settings` 里的键。
const PUSH_STATE_KEY: &str = "question.push_state";

/// 推一次的结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PushOutcome {
    /// 推出去了：这张卡已经算"已问"。
    Pushed { question: super::SelectedQuestion },
    /// 今天的次数用完了（或作者设成 0 = 不打扰）。
    QuotaUsed,
    /// 离上一次太近。
    TooSoon,
    /// **这一章还是空的，而这条时机在空章里不成立**（"你卡住了" / "这一章写完了"）——
    /// 见 [`crate::question::push::decide`]。**静默**：码只进日志与命令行输出，
    /// 界面只在 `Pushed` 时冒头（错过一次推本来就不该让作者知道）。
    EmptyChapter,
    /// 池子里没有可问的（一本书问完了，或全被静音/舍弃了）。
    NothingToAsk,
}

impl PushOutcome {
    /// 稳定码（进日志 / 命令行；界面不必显示——**错过一次推是静默的**）。
    pub fn as_str(&self) -> &'static str {
        match self {
            PushOutcome::Pushed { .. } => "push.asked",
            PushOutcome::QuotaUsed => PushDecision::QuotaUsed.as_str(),
            PushOutcome::TooSoon => PushDecision::TooSoon.as_str(),
            PushOutcome::EmptyChapter => PushDecision::EmptyChapter.as_str(),
            PushOutcome::NothingToAsk => "push.nothing_to_ask",
        }
    }
}

/// 读今天推了几次（坏记录当没推过——"宁可多问一次"这条与别处一致）。
fn read_push_state(conn: &Connection) -> PushState {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![PUSH_STATE_KEY], |r| r.get(0))
        .ok();
    raw.and_then(|text| serde_json::from_str::<PushState>(&text).ok()).unwrap_or_default()
}

fn write_push_state(conn: &Connection, state: &PushState) -> Result<()> {
    let json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
        params![PUSH_STATE_KEY, json, now_millis()],
    )?;
    Ok(())
}

impl Store {
    /// 主动问一句：过了门槛就从候选里挑一张问出去，并记下这一笔。
    ///
    /// `node_id` 给 `Some` 就是"跟着这一章走"的口径（当前章的问题优先）；
    /// `reason` 是**为什么现在开口**（核心的枚举，见 [`PushReason`]），
    /// 它进 op-log 的 trigger，事后一眼看得出这一次是哪条时机触发的。
    /// `today` 是作者本地那一天（`YYYYMMDD` 整数）——调用方按本地时区算好传进来。
    ///
    /// **空章不问**这条判据在这里落地：`decide` 要一个"当前这一章有没有正文"，
    /// 而这个事实只有存储层读得出来（`list_nodes` 顺手就有）——机制层照样不碰 SQL。
    /// 没打开任何一章（`node_id` 是 `None`）时按"没有正文"算：那两句话（"你卡住了" /
    /// "这一章写完了"）在"没有正在写的章"时同样不成立。
    pub fn push_question(
        &mut self,
        work_id: i64,
        node_id: Option<i64>,
        quota: PushQuota,
        today: i64,
        reason: PushReason,
    ) -> Result<PushOutcome> {
        let now = now_millis();
        let state = read_push_state(&self.conn);
        let has_body = self.chapter_has_body(work_id, node_id)?;
        match decide(&state, &quota, today, now, reason, has_body) {
            PushDecision::QuotaUsed => return Ok(PushOutcome::QuotaUsed),
            PushDecision::TooSoon => return Ok(PushOutcome::TooSoon),
            PushDecision::EmptyChapter => return Ok(PushOutcome::EmptyChapter),
            PushDecision::Ask => {}
        }

        // 挑一张：与面板同一套选题（跟着这一章走时，这一章的问题优先），取最靠前的那张
        let picked = match node_id {
            Some(node) => self.select_questions_for_chapter(work_id, node, 1)?,
            None => self.select_questions(work_id, 1)?,
        };
        let Some(question) = picked.into_iter().next() else {
            return Ok(PushOutcome::NothingToAsk);
        };

        // 算问过 + 记账：**同一个事务**
        let card = self.question_card(question.card_id)?;
        let tx = self.conn.transaction()?;
        move_card_in(
            &tx,
            &self.device_id,
            &card,
            QuestionState::Asked,
            &format!("push:{}", reason.as_str()),
        )?;
        write_push_state(&tx, &crate::question::advanced(&state, today, now))?;
        tx.commit()?;
        Ok(PushOutcome::Pushed { question })
    }

    /// 这一章有没有正文（判"空章不问"用）。
    ///
    /// 读法与 `writing_elements` 同一份目录（`list_nodes`：节点在不在、有没有正文），
    /// 不额外查一次库、也不新加一个"有没有正文"的 SQL——两处口径本来就该是同一个。
    /// 节点不在（**没打开任何一章**、或者 id 认不得／不是这本书的）就是 `false`。
    fn chapter_has_body(&self, work_id: i64, node_id: Option<i64>) -> Result<bool> {
        let Some(node) = node_id else { return Ok(false) };
        Ok(self
            .list_nodes(work_id)?
            .into_iter()
            .find(|summary| summary.id == node)
            .is_some_and(|summary| summary.has_body))
    }

    /// 今天已经主动问了几次（界面显示"今天问过 N 次"用；坏记录当 0）。
    pub fn pushed_today(&self, today: i64) -> Result<i64> {
        let state = read_push_state(&self.conn);
        Ok(if state.day == today { state.count } else { 0 })
    }
}
