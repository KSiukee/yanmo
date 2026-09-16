//! 作答：作者给了答案——把答案落成一张带溯源的碎片，并让问题卡走到终态。
//!
//! 三条分寸（与状态机、与创作留痕那条线对齐）：
//!
//! 1. **文本与输入方式解耦**：答案文本是答案文本，`source`（`typed` / `voice` / `mixed`）
//!    是另一个维度——同一条答案用键盘敲、用口述转写、还是先用嘴说再用手改，都落得下来，
//!    而且以后按 `source` 算口径时算得准（见 [`crate::model::InputSource`]）；
//! 2. **答案与状态同一个事务**：写答案碎片与把卡迁到「已答」是一件事的两半，
//!    分两次写就会留下"卡已答、答案没了"这种最难查的半截状态；
//! 3. **作答即视为问出**：作者没点开就直接答了（队列里一眼看到就答），
//!    这里补一条「问出」再答——新颖度从那刻才算消耗。不补的话库里会出现
//!    一张"从没被问过却已答"的卡，那是把账做平了，不是真话。
//!
//! 答案碎片住在**碎片统一表**（`frag_kind = 'answer'`）里，与问题卡、灵感速记同表，
//! 于是天生带 `source / created_at / linked / derived_from` 这一套（"答案池同表"）。
//! 它**不动正文一个字**：把答案落到章里是两条落点模式的事——
//! **穿插式**（[`Store::mark_answer_landed`]：答一条落一条，边想边写）
//! 与**先问后排版**（[`Store::apply_answer_round`]：一轮问完，一次落）。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::card_move::move_card_in;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{InputSource, QuestionCard, QuestionState};
use crate::time::now_millis;

/// 答案卡在碎片统一表里的种类码（写进 `fragments.frag_kind`；**别改**）。
const KIND_ANSWER: &str = "answer";

/// 一张答案卡（作者对某个问题给出的那一句）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Answer {
    pub id: i64,
    pub work_id: i64,
    /// 答的是哪张问题卡（就是它的 `derived_from`——"这条答案从哪来"永远查得到）
    pub card_id: i64,
    pub body: String,
    /// 怎么打出来的：`typed` / `voice` / `mixed`（创作留痕的原始素材）
    pub source: String,
    /// 这条答案的处置：`pending` = 躺在答案池里，`landed` = 落进过正文（见 [`Store::mark_answer_landed`]）
    pub status: String,
    pub created_at: i64,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Option<i64>, String, String, String, Option<i64>, i64)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn into_answer(raw: (i64, Option<i64>, String, String, String, Option<i64>, i64)) -> Answer {
    Answer {
        id: raw.0,
        // 与问题卡同一条口径：库里被手改成没有归属的按作品 0 算（如实显示，不假装有主）
        work_id: raw.1.unwrap_or(0),
        // 溯源断了的（库被手改）按"不知道答的是哪张"算，不编一个卡号出来
        card_id: raw.5.unwrap_or(0),
        body: raw.2,
        source: raw.3,
        status: raw.4,
        created_at: raw.6,
    }
}

/// 一轮里的一条：答的是哪张卡、作者最终认定的那段字（"先问后排版"用）。
///
/// 作者在这一轮里**可能改过字**（答完当场发现串行了、或者顺手润了一句）——所以这里给的
/// `body` 是要落下去的那一份，与库里不同就回写答案池（改了什么也留痕，见
/// [`Store::apply_answer_round`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundItem {
    pub card_id: i64,
    pub body: String,
}

impl Store {
    /// 作答：把答案落成碎片，并把问题卡迁到「已答」（终态）——**同一个事务**。
    ///
    /// 校验都在写之前：答案不许空、输入方式只认 `typed` / `voice` / `mixed`（认不出就报错，
    /// 不静默收下——留痕里的一列写歪了，比少一条记录坏得多）。
    ///
    /// 卡不在「待问 / 已问」态时由状态机拒掉：已答的再答、舍弃或静音过的拿来答，
    /// 都会被 [`crate::model::transition`] 挡下并报 `card.illegal_transition`。
    pub fn record_question_answer(
        &mut self,
        card_id: i64,
        body: &str,
        source: &str,
        trigger: &str,
    ) -> Result<i64> {
        let body = body.trim();
        if body.is_empty() {
            return Err(Error::invalid(codes::ANSWER_BODY_EMPTY));
        }
        let source = InputSource::parse(source)?;
        let card: QuestionCard = self.question_card(card_id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        if card.state == QuestionState::Pending {
            // 作者一眼看到就答了：先补一条「问出」（新颖度从这一刻才算消耗）
            move_card_in(&tx, &self.device_id, &card, QuestionState::Asked, trigger)?;
        }
        tx.execute(
            "INSERT INTO fragments(work_id, frag_kind, body, source, status, used_count,
                                   last_asked_at, linked, derived_from, auto_derived, template_key,
                                   created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 0, NULL, ?6, ?7, 0, '', ?8, ?8)",
            params![
                card.work_id,
                KIND_ANSWER,
                body,
                source.as_str(),
                // 碎片的 status 语义归创作流那条线（落进正文没有、用没用过）；
                // 这里给一个中性值，别把问题态混进碎片态
                "pending",
                card.linked,
                card_id,
                now
            ],
        )?;
        let answer_id = tx.last_insert_rowid();
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            answer_id,
            "create",
            json!({
                "kind": KIND_ANSWER,
                "to": QuestionState::Answered.as_str(),
                "trigger": trigger,
                "source": source.as_str(),
                "derived_from": card_id,
            }),
        )?;
        // 状态收尾：读到的卡可能是「待问」（上面刚补过「问出」），迁移要按**库里现在的态**走
        let asked = QuestionCard { state: QuestionState::Asked, ..card.clone() };
        move_card_in(&tx, &self.device_id, &asked, QuestionState::Answered, trigger)?;
        tx.commit()?;
        Ok(answer_id)
    }

    /// 把一条答案标成「落进过正文」——**只留痕，不写正文**。
    ///
    /// 正文里那一段字是**作者自己的字**：界面把答案插进编辑会话（于是自动落盘、字数、
    /// 码字账本、版本快照全照常走）。核心在这里做的是另一件事——记下"这一条用掉了、
    /// 落到哪一章"。分开的两条理由：
    ///
    /// 1. 击键级的正文写入必须留在壳内的编辑会话里（详见架构铁律）；核心直接改正文，
    ///    编辑器手里那一份就跟库里的分家了；
    /// 2. 留痕是核心的事：一条 op-log 从一处写，界面不自己编记录。
    ///
    /// 落点校验：得是**同一本书里、承载正文的节点**。允许反复落（作者觉得这一句还想
    /// 再放一次）：每落一次记一条，历史看得见——标记只说"落过"，不断言正文里现在还有。
    pub fn mark_answer_landed(&mut self, card_id: i64, node_id: i64, trigger: &str) -> Result<i64> {
        let card: QuestionCard = self.question_card(card_id)?;
        let answer = self.answer_of_question(card_id)?;
        check_landing_node(self, card.work_id, node_id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE fragments SET status = 'landed', updated_at = ?1
              WHERE id = ?2 AND frag_kind = ?3 AND deleted_at IS NULL",
            params![now, answer.id, KIND_ANSWER],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            answer.id,
            "land",
            json!({
                "card_id": card_id,
                "node_id": node_id,
                "work_id": card.work_id,
                "trigger": trigger,
            }),
        )?;
        tx.commit()?;
        Ok(answer.id)
    }

    /// 把**一轮**答案落进某一章（先问后排版）：回写改动 + 标已落 + 逐条留痕，**一个事务**。
    ///
    /// 与单条落章（[`Store::mark_answer_landed`]）的分工：那条是"答一条落一条"，
    /// 这条是"一轮问完一次落"。两条路的留痕落在同一条链上（都是 fragment 的 op-log），
    /// 只是这条一次记一串——落下去的顺序**就是这里给的顺序**（作者在托盘里排的那个）。
    ///
    /// 正文那几段字**不由核心写**：界面把它们一次插进编辑会话（于是自动落盘、字数、账本、
    /// 版本快照全照常）。核心在这儿做的是账。
    ///
    /// 校验在写之前一次做完：章得是同一本书里承载正文的节点；每条都得真的答过、
    /// **不属于这本书的卡当场拒绝**——半轮落下去比整轮不落更难查。
    pub fn apply_answer_round(
        &mut self,
        work_id: i64,
        node_id: i64,
        items: &[RoundItem],
        trigger: &str,
    ) -> Result<Vec<i64>> {
        if items.is_empty() {
            return Err(Error::invalid(codes::ROUND_EMPTY));
        }
        check_landing_node(self, work_id, node_id)?;
        // 读侧先做完（要拿每条的现状：有没有答案、字改没改），写侧只开一个事务
        let mut planned = Vec::new();
        for item in items {
            let card: QuestionCard = self.question_card(item.card_id)?;
            if card.work_id != work_id {
                return Err(Error::invalid_with(
                    codes::ROUND_CARD_FOREIGN,
                    [
                        ("card_id", item.card_id.to_string()),
                        ("work_id", work_id.to_string()),
                    ],
                ));
            }
            let body = item.body.trim();
            if body.is_empty() {
                return Err(Error::invalid(codes::ANSWER_BODY_EMPTY));
            }
            let answer = self.answer_of_question(item.card_id)?;
            planned.push((card.id, answer, body.to_string()));
        }

        let now = now_millis();
        let tx = self.conn.transaction()?;
        let mut landed = Vec::new();
        for (card_id, answer, body) in planned {
            // 作者改过字：回写答案池，并留下"改了"这一笔（改前改后各有几个字，别只留个 flag）
            if answer.body != body {
                tx.execute(
                    "UPDATE fragments SET body = ?1, updated_at = ?2
                      WHERE id = ?3 AND frag_kind = ?4 AND deleted_at IS NULL",
                    params![body, now, answer.id, KIND_ANSWER],
                )?;
                Self::record_in(
                    &self.device_id,
                    &tx,
                    "fragments",
                    answer.id,
                    "amend",
                    json!({
                        "chars_before": answer.body.chars().count(),
                        "chars": body.chars().count(),
                        "trigger": trigger,
                    }),
                )?;
            }
            tx.execute(
                "UPDATE fragments SET status = 'landed', updated_at = ?1
                  WHERE id = ?2 AND frag_kind = ?3 AND deleted_at IS NULL",
                params![now, answer.id, KIND_ANSWER],
            )?;
            Self::record_in(
                &self.device_id,
                &tx,
                "fragments",
                answer.id,
                "land",
                json!({
                    "card_id": card_id,
                    "node_id": node_id,
                    "work_id": work_id,
                    "trigger": trigger,
                    "round": true,
                }),
            )?;
            landed.push(answer.id);
        }
        tx.commit()?;
        Ok(landed)
    }

    /// 取一张问题卡上的答案（已答的卡一定有）。
    ///
    /// 一张卡是终态，正常只有一条；库里被手改成多条时给**最近那条**，不装作没看见。
    pub fn answer_of_question(&self, card_id: i64) -> Result<Answer> {
        let raw = self
            .conn
            .query_row(
                "SELECT id, work_id, body, source, status, derived_from, created_at FROM fragments
                  WHERE derived_from = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
                  ORDER BY created_at DESC, id DESC LIMIT 1",
                params![card_id, KIND_ANSWER],
                read_row,
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::ANSWER_NOT_FOUND, [("card_id", card_id.to_string())])
            })?;
        Ok(into_answer(raw))
    }

    /// 一张问题卡上的全部答案（按答下的先后；正常只有一条）。
    pub fn answers_of_question(&self, card_id: i64) -> Result<Vec<Answer>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, work_id, body, source, status, derived_from, created_at FROM fragments
              WHERE derived_from = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
              ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![card_id, KIND_ANSWER], read_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_answer(row?));
        }
        Ok(out)
    }
}

/// 落点校验：那个节点得**存在、属于这本书、且承载正文**。
///
/// 三种不对分别报错（"没了"用既有的 `node.gone`；"不是这本书的 / 不是正文节点"合成一条，
/// 因为对作者来说是同一件事：这一段放不了）。节点那一列的读法走 [`super::node`]，
/// 不在这边另写一份 SELECT。
fn check_landing_node(store: &Store, work_id: i64, node_id: i64) -> Result<()> {
    let owner = super::node::node_work_in(store.conn(), node_id)?;
    let holds_body = super::node::node_kind_in(store.conn(), node_id)?.holds_body();
    if owner != work_id || !holds_body {
        return Err(Error::invalid_with(
            codes::ANSWER_LAND_NODE_INVALID,
            [("node_id", node_id.to_string())],
        ));
    }
    Ok(())
}
