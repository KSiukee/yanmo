//! 问题卡的**状态迁移**：每条边都是一次可核对的事件。
//!
//! 迁移表在 [`crate::model::card`]（纯逻辑），这里只做三件事：
//! 查合法边 → 同事务改 `fragments.status` + 留一条 op-log → 返回原状态。
//!
//! 为什么要留痕、留什么：验收要求"每条迁移都有可观测证据（**谁触发、落在哪张表/哪个字段**）"。
//! 所以每次迁移写两处——
//! - 落在**字段**：`fragments.status`（新态）、`fragments.updated_at`；
//!   问出（`ask`）还额外把 `fragments.used_count` +1，那是"新颖度冷却"的账；
//! - 落在**事件**：op-log 一行，`entity = fragments`、`op` = 动作码（ask / answer / defer /
//!   discard / mute / requeue / retrieve / unmute）、payload = `{from, to, trigger}`。
//!   其中 `trigger` 由调用方给（push / pull / author / system / 模块名），audit 时一眼看得出
//!   这一步是系统弹的还是作者自己点的。

use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::{json, Value};

use super::card::KIND_QUESTION;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::gravity::signal_for_action;
use crate::model::{transition, QuestionCard, QuestionState};
use crate::time::now_millis;

/// 一条问题卡事件（从 op-log 读回来）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CardEvent {
    /// 动作码：`create`，或迁移表里的 ask / answer / …
    pub op: String,
    /// 从哪个态来（`create` 没有 from，是空串）
    pub from: String,
    /// 到哪个态去
    pub to: String,
    /// 谁触发（push / pull / author / system / 模块名）
    pub trigger: String,
    /// 发生时间（毫秒）
    pub at: i64,
}

/// 从 op-log 的 payload 里取一个字符串字段（坏 payload 当空串，不连累整条历史）。
fn field(payload: &Value, key: &str) -> String {
    payload.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

impl Store {
    /// 把一张卡从**当前**状态迁到 `to`。
    ///
    /// - 非法边当场拒绝（`card.illegal_transition`），一个字节都不写；
    /// - `ask` 让 `used_count` +1（新颖度冷却靠它，见 [`crate::model::QuestionCard::used_count`]）；
    /// - 更新条件是"当前状态正好是刚才读到的那个"（compare-and-swap）：万一有别的进程
    ///   在中间改了库，这里会明确报错，而不是把一条非法边悄悄写下去。
    ///
    /// 返回迁移前的状态（调用方常用它说"从 X 到 Y"）。
    pub fn move_question_card(
        &mut self,
        id: i64,
        to: QuestionState,
        trigger: &str,
    ) -> Result<QuestionState> {
        let card = self.question_card(id)?;
        let from = card.state;
        let tx = self.conn.transaction()?;
        move_card_in(&tx, &self.device_id, &card, to, trigger)?;
        tx.commit()?;
        Ok(from)
    }

    /// 一张卡的全部事件（含建卡那条），按发生顺序。
    pub fn card_events(&self, id: i64) -> Result<Vec<CardEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT op, payload, created_at FROM op_log
              WHERE entity = 'fragments' AND entity_id = ?1
              ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (op, payload, at) = row?;
            let payload: Value = serde_json::from_str(&payload).unwrap_or(Value::Null);
            out.push(CardEvent {
                from: field(&payload, "from"),
                to: field(&payload, "to"),
                trigger: field(&payload, "trigger"),
                op,
                at,
            });
        }
        Ok(out)
    }
}

/// 事务内的迁移：改状态字段 + 留痕 + "处置即教学"。
///
/// **调用方负责开事务与提交**——延后（要同时写条件）与重出（要同时标掉那条条件）
/// 都靠它把"状态"与"跟着它一起该落的东西"放进**同一个事务**，
/// 不然就会出现"卡改了态、条件没写上"这类半截状态（那正是最难查的一种）。
pub(super) fn move_card_in(
    tx: &Transaction<'_>,
    device_id: &str,
    card: &QuestionCard,
    to: QuestionState,
    trigger: &str,
) -> Result<()> {
    let from = card.state;
    let edge = transition(from, to).ok_or_else(|| illegal(from, to))?;
    let now = now_millis();
    let affected = if edge.action == "ask" {
        // 问出：记下次数**与时刻**——新颖度冷却要靠这两样（次数管多少回、时刻管多久以前）
        tx.execute(
            "UPDATE fragments SET status = ?1, used_count = used_count + 1,
                                  last_asked_at = ?2, updated_at = ?2
              WHERE id = ?3 AND frag_kind = ?4 AND deleted_at IS NULL AND status = ?5",
            params![to.as_str(), now, card.id, KIND_QUESTION, from.as_str()],
        )?
    } else {
        tx.execute(
            "UPDATE fragments SET status = ?1, updated_at = ?2
              WHERE id = ?3 AND frag_kind = ?4 AND deleted_at IS NULL AND status = ?5",
            params![to.as_str(), now, card.id, KIND_QUESTION, from.as_str()],
        )?
    };
    if affected == 0 {
        // 卡没了，或状态在我们读它与写它之间被人改了——两种都要说清是哪一种
        let current: Option<String> = tx
            .query_row(
                "SELECT status FROM fragments
                  WHERE id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL",
                params![card.id, KIND_QUESTION],
                |r| r.get(0),
            )
            .optional()?;
        return match current {
            None => Err(Error::invalid_with(
                codes::CARD_NOT_FOUND,
                [("card_id", card.id.to_string())],
            )),
            Some(state) => Err(illegal_current(&state, to)),
        };
    }
    Store::record_in(
        device_id,
        tx,
        "fragments",
        card.id,
        edge.action,
        json!({ "from": from.as_str(), "to": to.as_str(), "trigger": trigger }),
    )?;
    // 处置**就是**教学：动作码折算成偏好信号，喂给同类模板（同一事务，漏不掉）
    if let Some(signal) = signal_for_action(edge.action) {
        super::question_weights::learn_in(tx, &card.template_key, signal, now)?;
    }
    Ok(())
}

/// 非法边：`from → to` 不在迁移表里。
fn illegal(from: QuestionState, to: QuestionState) -> Error {
    Error::invalid_with(
        codes::CARD_ILLEGAL_TRANSITION,
        [("from", from.as_str().to_string()), ("to", to.as_str().to_string())],
    )
}

/// 迁移前读到的状态与库里的**实际**状态对不上时的同一条错（报真实的那一端）。
fn illegal_current(current: &str, to: QuestionState) -> Error {
    Error::invalid_with(
        codes::CARD_ILLEGAL_TRANSITION,
        [("from", current.to_string()), ("to", to.as_str().to_string())],
    )
}
