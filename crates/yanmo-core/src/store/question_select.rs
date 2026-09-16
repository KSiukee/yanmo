//! 选题：把候选池排出一个顺序来（**只读**）。
//!
//! 「作者主动打开叩问」不受任何配额限制，看多少问题都不算"已问"、不消耗新颖度——
//! 所以这个入口**只读**：不写库、不改状态。真正"问出来"是另一个动作
//! （[`super::card_move`] 的 `ask`，那一刻才记次数与时刻，也才进冷却）。
//!
//! 池子 = **待问**（pending）的卡。已问 / 已答 / 延后 / 舍弃 / 静音都不在池子里；
//! 「延后的重出」要等条件满足才回池子——那是延后队列那件事，不在这里硬插队。

use std::collections::HashMap;

use serde::Serialize;

use super::{question_weights, Store};
use crate::error::Result;
use crate::gravity::{gravity, rank, AttractorParams, Candidate, Gravity};
use crate::model::{QuestionCard, QuestionState};
use crate::question::{anchors_of, urgency_of};
use crate::time::now_millis;

/// 选出来的一条问题：卡 + **引力拆解**（将来界面能回答"为什么先问这个"）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectedQuestion {
    pub card_id: i64,
    pub template_key: String,
    pub body: String,
    /// 卡上的关联锚点（`chapter:12` 这种）：界面靠它认"这条问的是不是这一章"
    pub anchors: Vec<String>,
    pub gravity: Gravity,
}

impl Store {
    /// 按引力排出一本书此刻该问的问题（最多 `limit` 条）。
    pub fn select_questions(&self, work_id: i64, limit: usize) -> Result<Vec<SelectedQuestion>> {
        self.select_questions_in(work_id, limit, None)
    }

    /// **跟着这一章走**的选题：与这一章有关的问题排最前，其余照旧按引力跟在后面。
    ///
    /// 为什么只"抬当前章"、不顺手把"下一章"也排上来：那是猜——机制并不知道作者接下来
    /// 写哪一章。作者换到哪一章，那一章的问题就自然浮到最前，"按章节顺序逐个问"是这么成立的。
    pub fn select_questions_for_chapter(
        &self,
        work_id: i64,
        node_id: i64,
        limit: usize,
    ) -> Result<Vec<SelectedQuestion>> {
        self.select_questions_in(work_id, limit, Some(node_id))
    }

    /// 选题本体：`prefer_node` 给了就把锚点落在那张节点上的**整层抬到最前**。
    fn select_questions_in(
        &self,
        work_id: i64,
        limit: usize,
        prefer_node: Option<i64>,
    ) -> Result<Vec<SelectedQuestion>> {
        let params = AttractorParams::default();
        let now = now_millis();
        // 被静音的来源整批不摆到作者面前（"这个模块太吵"→只让它闭嘴，不关掉整个叩问）
        let muted = self.muted_sources()?;
        let cards: Vec<_> = self
            .question_cards(work_id, Some(QuestionState::Pending))?
            .into_iter()
            .filter(|card| !muted.iter().any(|source| source == &card.source))
            .collect();
        // 防死循环的账：这张卡被延后过几次（一次分组查询，不做 N+1）
        let deferred = self.defer_counts(work_id)?;
        let index: HashMap<i64, usize> =
            cards.iter().enumerate().map(|(at, card)| (card.id, at)).collect();

        let mut scored = Vec::new();
        for card in &cards {
            let learned = if card.template_key.is_empty() {
                Default::default()
            } else {
                question_weights::read_weight(&self.conn, &card.template_key)?
            };
            let candidate = Candidate {
                card_id: card.id,
                base_urgency: urgency_of(&card.template_key),
                importance: card.importance,
                used_count: card.used_count,
                last_asked_at: card.last_asked_at,
                auto_derived: card.auto_derived,
                defer_count: deferred.get(&card.id).copied().unwrap_or(0) as usize,
                template_weight: learned.weight,
                template_muted: !learned.enabled,
            };
            scored.push((card.id, gravity(&candidate, now, &params)));
        }

        // 静音的那一类引力为 0：**滤掉**（不是排到后面——"这一类以后都别问了"是不出现）
        let ranked: Vec<(i64, Gravity)> =
            rank(scored).into_iter().filter(|(_, gravity)| gravity.template_weight > 0.0).collect();
        let ordered = match prefer_node {
            // 跟着这一章走：锚点落在这一章的整层抬到最前，层内仍是引力序（不重排机制给的顺序）
            Some(node) => {
                let anchored = |card_id: i64| {
                    anchors_of(&cards[index[&card_id]].linked)
                        .iter()
                        .any(|anchor| anchor == &format!("chapter:{node}"))
                };
                let (mine, rest): (Vec<_>, Vec<_>) =
                    ranked.iter().partition(|(card_id, _)| anchored(*card_id));
                mine.into_iter().chain(rest).cloned().collect()
            }
            None => ranked,
        };
        // 同类不扎堆（见 [`PER_TEMPLATE_IN_SCREEN`]）：同类里靠后的几条排到别的类后面去，
        // **一条都不丢**——只是这一屏先不摆它们
        let spread = spread_by_template(&ordered, &cards, &index);
        Ok(spread
            .into_iter()
            .take(limit)
            .map(|(card_id, gravity)| {
                let card = &cards[index[&card_id]];
                SelectedQuestion {
                    card_id,
                    template_key: card.template_key.clone(),
                    body: card.body.clone(),
                    anchors: anchors_of(&card.linked),
                    gravity,
                }
            })
            .collect())
    }

    /// 候选池里还有多少条（**同一套口径**：待问的卡，且不是被静音的来源）——不限条数。
    ///
    /// 界面拿它说"池子里还有 N 条在排着"：一屏只摆得下几条，作者得知道**底下还有**，
    /// 而不是以为"能问的就这一条"。
    pub fn count_pending_questions(&self, work_id: i64) -> Result<usize> {
        let muted = self.muted_sources()?;
        Ok(self
            .question_cards(work_id, Some(QuestionState::Pending))?
            .into_iter()
            .filter(|card| !muted.iter().any(|source| source == &card.source))
            .count())
    }
}

/// 一屏里**同一类（模板）最多摆几条**。
///
/// 为什么不摆满：一个状态会在书里重复出现——比如十来个空章，"这一章从哪儿开始"就会产出十来条
/// 一模一样的问题，只差章名。一屏全是同一件事，作者会以为机制只会问这一句。
/// 同类先摆一条，其余排到别的类后面（不是丢掉：一屏之外还排着，答完一条下一条就浮上来）。
///
/// 不设成 0：同类里最靠前的那条仍然该摆出来——它可能就是此刻最该问的那一条。
const PER_TEMPLATE_IN_SCREEN: usize = 1;

/// 同类不扎堆：稳定地重排一遍（前段每类最多 `PER_TEMPLATE_IN_SCREEN` 条，其余按原序接在后面）。
///
/// 作者自己写的卡与模块提交的卡 `template_key` 是空串——它们**不进这个帽子**
/// （空串不是"一类"，把它们挤成一条就成了另一种假象）。
fn spread_by_template(
    ordered: &[(i64, Gravity)],
    cards: &[QuestionCard],
    index: &HashMap<i64, usize>,
) -> Vec<(i64, Gravity)> {
    let mut head = Vec::new();
    let mut rest = Vec::new();
    let mut taken: HashMap<&str, usize> = HashMap::new();
    for (card_id, gravity) in ordered {
        let key = cards[index[card_id]].template_key.as_str();
        let slot = taken.entry(key).or_insert(0);
        if key.is_empty() || *slot < PER_TEMPLATE_IN_SCREEN {
            *slot += 1;
            head.push((*card_id, gravity.clone()));
        } else {
            rest.push((*card_id, gravity.clone()));
        }
    }
    head.extend(rest);
    head
}
