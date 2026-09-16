//! 落章：把答案送到该去的地方——**正文段落 / 章纲（这一章的一句话）/ 场景卡（新建一张）**。
//!
//! 与 [`super::question_answer`]（作答：把答案记下来）分开的原因：那半边只管"记下来"，
//! 这半边管"送到哪儿去"——两件事的变化理由不一样（一个跟着答案池长，一个跟着落点长）。
//!
//! # 三条分寸
//!
//! 1. **正文那几段字不由核心写**：界面把它插进编辑会话（于是自动落盘、字数、码字账本、
//!    版本快照全照常走）——击键级的正文写入必须留在壳内的编辑会话里，核心直接改正文
//!    会让编辑器手里那一份跟库里的分家。核心在这儿做的是账；
//! 2. **章纲与场景卡由核心写**：那是低频的结构改动（写 `nodes.summary` / 在节点树里新建一张），
//!    不是击键级；三种落点与"标已落 + 留痕"**同一个事务**，不会有半截状态；
//! 3. **落错地方比落不下去难查**：认不出的落点当场拒；一条不对就整轮不写。
//!
//! 两条落法共用同一个实现（[`Store::land_items`]）：**穿插式**（[`Store::mark_answer_landed`]：
//! 答一条落一条，边想边写）与**先问后排版**（[`Store::apply_answer_round`]：一轮问完，一次落）。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::question_answer::{Answer, KIND_ANSWER};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{AnswerTarget, NodeKind, QuestionCard};
use crate::time::now_millis;

/// 一轮里的一条：答的是哪张卡、作者最终认定的那段字（"先问后排版"用）。
///
/// 作者在这一轮里**可能改过字**（答完当场发现串行了、或者顺手润了一句）——所以这里给的
/// `body` 是要落下去的那一份，与库里不同就回写答案池（改了什么也留痕，见
/// [`Store::apply_answer_round`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundItem {
    pub card_id: i64,
    pub body: String,
    /// 落到哪儿：`body` / `outline` / `scene`；**空串当 `body`**（不带这一栏的老调用照旧落正文）。
    /// 认不出的取值当场拒——落错地方比落不下去难查。
    #[serde(default)]
    pub target: String,
    /// 场景卡的标题（只对 `scene` 有意义）；空串＝留给作者自己在树上起名。
    #[serde(default)]
    pub title: String,
}

/// 一次落章的结果（核心记下的账）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LandReceipt {
    /// 落下去的答案 id（按给的顺序）
    pub answer_ids: Vec<i64>,
    /// 新建的场景卡 id（按给的顺序里 `scene` 的那几条）
    pub scene_ids: Vec<i64>,
    /// 章纲最后成了什么（这一次没落章纲就是 `None`）——
    /// 界面拿它更新正文上方那一栏"一句话"，更新的是**库里的真值**，不是自己猜的。
    pub outline: Option<String>,
}

impl Store {
    /// 落一条答案（就地落）：**正文 / 章纲 / 场景卡三种都走它**。
    ///
    /// `target`：`body`（正文段落，默认）/ `outline`（章纲：这一章的一句话）/
    /// `scene`（场景卡：这一章下面新建一张，`title` 是它的名字，空着留给作者起）。
    ///
    /// **正文那一段字不由核心写**：界面把它插进编辑会话（于是自动落盘、字数、码字账本、
    /// 版本快照全照常走）——击键级的正文写入必须留在壳内的编辑会话里，核心直接改正文
    /// 会让编辑器手里那一份跟库里的分家。核心在这里做的是账。
    ///
    /// 章纲与场景卡则**由核心写**（那是低频的结构改动，不是击键级）：章纲写进
    /// `nodes.summary`，场景卡在节点树里新建一张。三种落点与"标已落 + 留痕"**同一个事务**，
    /// 不会有半截状态。
    ///
    /// 允许反复落（作者觉得这一句还想再放一次）：每落一次记一条，历史看得见——
    /// 标记只说"落过"，不断言正文里现在还有。
    pub fn mark_answer_landed(
        &mut self,
        card_id: i64,
        node_id: i64,
        target: &str,
        title: &str,
        trigger: &str,
    ) -> Result<LandReceipt> {
        let card: QuestionCard = self.question_card(card_id)?;
        let answer = self.answer_of_question(card_id)?;
        let item = RoundItem {
            card_id,
            body: answer.body,
            target: target.to_string(),
            title: title.to_string(),
        };
        self.land_items(card.work_id, node_id, &[item], trigger, false)
    }

    /// 把**一轮**答案落进某一章（先问后排版）：落点各异、一次一串，**一个事务**。
    ///
    /// 与单条落章的分别：那条是"答一条落一条"，这条是"一轮问完一次落"。两条路的留痕落在
    /// 同一条链上（都是 fragment 的 op-log），只是这条一次记一串——落下去的顺序
    /// **就是这里给的顺序**（作者在托盘里排的那个）。
    ///
    /// 校验在写之前一次做完：章得是同一本书里承载正文的节点；每条都得真的答过、
    /// **不属于这本书的卡当场拒绝**——半轮落下去比整轮不落更难查。
    pub fn apply_answer_round(
        &mut self,
        work_id: i64,
        node_id: i64,
        items: &[RoundItem],
        trigger: &str,
    ) -> Result<LandReceipt> {
        if items.is_empty() {
            return Err(Error::invalid(codes::ROUND_EMPTY));
        }
        self.land_items(work_id, node_id, items, trigger, true)
    }

    /// 落章的**唯一实现**：校验 → 章纲合并 / 场景卡新建 / 回写改动 → 标已落 + 逐条留痕。
    ///
    /// `round` 只影响留痕里的一个标记（这一笔是"一轮里的第几条"还是单独落的），
    /// 落法本身没有第二种。
    fn land_items(
        &mut self,
        work_id: i64,
        node_id: i64,
        items: &[RoundItem],
        trigger: &str,
        round: bool,
    ) -> Result<LandReceipt> {
        check_landing_node(self, work_id, node_id)?;
        // 读侧先做完（要拿每条的现状：有没有答案、字改没改、落点认不认得出），写侧只开一个事务
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
            let target = AnswerTarget::from_wire(&item.target)?;
            let answer = self.answer_of_question(item.card_id)?;
            planned.push((card.id, answer, body.to_string(), target, item.title.clone()));
        }

        // 章纲：**合并成一行**（原有的 + 新的，用「；」接）
        let outline = merged_outline(self, node_id, &planned)?;
        // 命名规则也在事务外先问好：事务里已经借着 `self.conn`，不能再借一次 `self`
        // （见 `node_edit::create_node_in` 的说明）
        let naming = self.naming_style(work_id)?;

        let now = now_millis();
        let tx = self.conn.transaction()?;
        // ① 章纲先落定（与答案的账同一个事务）
        if let Some(text) = &outline {
            super::node_edit::set_summary_in(&tx, node_id, text)?;
            Self::record_in(
                &self.device_id,
                &tx,
                "nodes",
                node_id,
                "set_summary",
                json!({ "chars": text.chars().count(), "from_questions": true }),
            )?;
        }
        let mut answer_ids = Vec::new();
        let mut scene_ids = Vec::new();
        for (card_id, answer, body, target, title) in planned {
            // ② 作者改过字：回写答案池，并留下"改了"这一笔（改前改后各有几个字）
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
            // ③ 场景卡：这一章下面新建一张，答案就是它的正文（建卡与正文同一笔）
            let scene_id = match target {
                AnswerTarget::Scene => {
                    let id = super::node_edit::create_node_in(
                        &tx,
                        work_id,
                        Some(node_id),
                        NodeKind::Scene,
                        &title,
                        naming,
                    )?;
                    let stats = super::content::stats_of(&body);
                    super::content::put_body(&tx, id, &body, stats)?;
                    Self::record_in(
                        &self.device_id,
                        &tx,
                        "nodes",
                        id,
                        "create",
                        json!({
                            "work_id": work_id,
                            "parent_id": node_id,
                            "kind": NodeKind::Scene.as_str(),
                            "chars": stats.char_count,
                            "from_question": card_id,
                        }),
                    )?;
                    scene_ids.push(id);
                    Some(id)
                }
                _ => None,
            };
            // ④ 答案的账：标已落 + 一条痕（写清落到哪儿；场景卡还记下新卡 id）
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
                    "target": target.as_str(),
                    "scene_id": scene_id,
                    "trigger": trigger,
                    "round": round,
                }),
            )?;
            answer_ids.push(answer.id);
        }
        tx.commit()?;
        Ok(LandReceipt { answer_ids, scene_ids, outline })
    }

}

/// 章纲那一行的接头：多条答案合到一行上，用「；」分开。
///
/// 为什么是"合并"而不是"覆盖"：那一句话上可能已经有作者**手写**的内容，覆盖就是默默弄丢它；
/// 而它本身是**单行输入框**（投稿包大纲也把它拼成一行 `标题 —— 一句话`），所以换行也不合适。
// i18n-allow-next-line: 这是**写进作者"一句话"里的连接符**（他的数据、随时能改），不是界面文案——
// 它跟着数据走、不随界面语言变（与默认章节名、磁盘目录名同一类）
const OUTLINE_JOINER: &str = "；";

/// 把这一轮里投章纲的那几条合到"这一章的一句话"上：原有在前、新的按顺序接在后面。
///
/// 返回 `None`＝这一轮没人投章纲（那就一个字都不动，连留痕也不留）。
/// **已经在那句话里的不重复接**：同一句落两回不该在章纲里出现两遍。
fn merged_outline(
    store: &Store,
    node_id: i64,
    planned: &[(i64, Answer, String, AnswerTarget, String)],
) -> Result<Option<String>> {
    let added: Vec<&str> = planned
        .iter()
        .filter(|(_, _, _, target, _)| *target == AnswerTarget::Outline)
        .map(|(_, _, body, _, _)| body.as_str())
        .collect();
    if added.is_empty() {
        return Ok(None);
    }
    let current: String = store
        .conn()
        .query_row(
            "SELECT summary FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
            params![node_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())]))?;
    let mut parts: Vec<String> =
        current.split(OUTLINE_JOINER).map(|piece| piece.trim().to_string()).filter(|p| !p.is_empty()).collect();
    for body in added {
        if !parts.iter().any(|piece| piece == body) {
            parts.push(body.to_string());
        }
    }
    Ok(Some(parts.join(OUTLINE_JOINER)))
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
