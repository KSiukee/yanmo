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
use crate::model::QuestionState;
use crate::question::urgency_of;
use crate::time::now_millis;

/// 选出来的一条问题：卡 + **引力拆解**（将来界面能回答"为什么先问这个"）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectedQuestion {
    pub card_id: i64,
    pub template_key: String,
    pub body: String,
    pub gravity: Gravity,
}

impl Store {
    /// 按引力排出一本书此刻该问的问题（最多 `limit` 条）。
    pub fn select_questions(&self, work_id: i64, limit: usize) -> Result<Vec<SelectedQuestion>> {
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

        Ok(rank(scored)
            .into_iter()
            // 静音的那一类引力为 0：**滤掉**（不是排到后面——"这一类以后都别问了"是不出现）
            .filter(|(_, gravity)| gravity.template_weight > 0.0)
            .take(limit)
            .map(|(card_id, gravity)| {
                let card = &cards[index[&card_id]];
                SelectedQuestion {
                    card_id,
                    template_key: card.template_key.clone(),
                    body: card.body.clone(),
                    gravity,
                }
            })
            .collect())
    }
}
