//! 问题卡的**生命周期状态机**：「叩问」问出的每一张卡能怎么走。
//!
//! 这个文件是**纯逻辑**：状态、迁移表、终态声明。没有 SQL、没有 IO、没有界面——
//! 所以"哪些态到得了、哪些态出得去"能被机械地验，而不是靠读代码印象。
//! 卡的数据字段在 [`super::card`]；把状态写进库在 [`crate::store::card_move`]。
//!
//! # 六个态与它们的边界
//!
//! ```text
//!                    ask              answer
//!   pending ─────────────▶ asked ─────────────▶ answered（终态）
//!      │  │                  │  │
//!      │  │ discard/mute     │  │ defer
//!      │  ▼                  │  ▼
//!      │ discarded ◀─────────┘   deferred
//!      │ muted                   │
//!      └──── retrieve / unmute / requeue ──▶ pending（重出 / 捞回 / 解除静音）
//! ```
//!
//! 三条不变量（`cargo test` 里有机械判据，见 `tests/cards.rs`）：
//!
//! 1. **进得来**：每个态都能从 `pending` 走到（不留"永远到不了的态"）；
//! 2. **出得去**：只有被 [`QuestionState::is_terminal`] 显式声明的终态可以没有出边；
//! 3. **有据可查**：每条边的动作码唯一，写库时落进 op-log（谁触发、从哪到哪）。

use crate::error::{codes, Error, Result};

/// 一条问题卡的生命周期状态。
///
/// 稳定代码写进库（`fragments.status`）——**别改**（改了老库的卡就认不出来了）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub enum QuestionState {
    /// 待问：进了候选池，还没被问出来。
    Pending,
    /// 已问：系统弹出过（push）或作者在叩问栏里翻到了并作答（pull）。
    Asked,
    /// 已答：作者给了答案——本轮的**终态**（答完的卡不再回池子；想再问是作者自己写新卡）。
    Answered,
    /// 延后：作者一时没想好。带重出条件回池子（条件与调度见后续的延后队列）。
    Deferred,
    /// 舍弃：进冷却库。**不真删**——可捞回，并作为选题负样本教系统少问这类。
    Discarded,
    /// 静音：这一类以后都别问了（可撤销）。按卡与按模块静音都落在这个态上。
    Muted,
}

impl QuestionState {
    /// 全部取值（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [QuestionState; 6] = [
        QuestionState::Pending,
        QuestionState::Asked,
        QuestionState::Answered,
        QuestionState::Deferred,
        QuestionState::Discarded,
        QuestionState::Muted,
    ];

    /// 稳定代码（写库 / 进 JSON；别改）。
    pub const fn as_str(self) -> &'static str {
        match self {
            QuestionState::Pending => "pending",
            QuestionState::Asked => "asked",
            QuestionState::Answered => "answered",
            QuestionState::Deferred => "deferred",
            QuestionState::Discarded => "discarded",
            QuestionState::Muted => "muted",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_question_state`。
    pub fn parse(s: &str) -> Result<Self> {
        QuestionState::ALL
            .into_iter()
            .find(|state| state.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_QUESTION_STATE, [("value", s.to_string())])
            })
    }

    /// 终态：没有出边是**有意的**，不是漏了迁移。
    ///
    /// 只有 `answered` 是终态——`discarded` / `muted` 都能捞回来 / 解除，
    /// `deferred` 等条件满足就重出（"每个态都出得去"这条不变量靠它才成立）。
    pub const fn is_terminal(self) -> bool {
        matches!(self, QuestionState::Answered)
    }

    /// 从这里出发的合法迁移（顺序即迁移表里的顺序）。
    pub fn next_states(self) -> Vec<QuestionState> {
        TRANSITIONS.iter().filter(|t| t.from == self).map(|t| t.to).collect()
    }
}

/// 一次合法迁移：从哪到哪、动作码叫什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub from: QuestionState,
    pub to: QuestionState,
    /// **稳定动作码**——它就是 op-log 里的 `op`（"谁触发"另有一个 `trigger` 参数）。
    pub action: &'static str,
}

/// 完整迁移表——**唯一的真相源**：允许怎么跳、跳的动作叫什么，都只写在这里。
///
/// 每条边上方的注释说的是"这条边为什么允许"（纯注释：既不当界面文案，也不进库）。
pub const TRANSITIONS: &[Transition] = &[
    // 问出来：系统主动弹出，或作者在叩问栏里翻到并作答
    Transition { from: QuestionState::Pending, to: QuestionState::Asked, action: "ask" },
    // 还没问就被舍弃（作者在队列里直接划掉）——进冷却库，可捞回
    Transition { from: QuestionState::Pending, to: QuestionState::Discarded, action: "discard" },
    // 这一类整体静音（按类 / 按模块）——这一类里还没问的卡一并静音
    Transition { from: QuestionState::Pending, to: QuestionState::Muted, action: "mute" },
    // 作者答了（键盘 / 口述 / 混用都算，输入方式记在答案卡上）
    Transition { from: QuestionState::Asked, to: QuestionState::Answered, action: "answer" },
    // 作者一时没想好——带重出条件回队列
    Transition { from: QuestionState::Asked, to: QuestionState::Deferred, action: "defer" },
    // 答不上来也不想要——进冷却库，可捞回
    Transition { from: QuestionState::Asked, to: QuestionState::Discarded, action: "discard" },
    // 这一类以后都别问了——立刻停掉，不留在队列里
    Transition { from: QuestionState::Asked, to: QuestionState::Muted, action: "mute" },
    // 延后的重出条件满足了——引力回升、重进候选池（不是硬插队）
    Transition { from: QuestionState::Deferred, to: QuestionState::Pending, action: "requeue" },
    // 从冷却库捞回（舍弃不真删的理由）
    Transition { from: QuestionState::Discarded, to: QuestionState::Pending, action: "retrieve" },
    // 解除静音（静音可选、可撤销、绝不默认开启）
    Transition { from: QuestionState::Muted, to: QuestionState::Pending, action: "unmute" },
];

/// 查一条边；没有就是非法迁移（调用方据此报 `card.illegal_transition`）。
pub fn transition(from: QuestionState, to: QuestionState) -> Option<&'static Transition> {
    TRANSITIONS.iter().find(|t| t.from == from && t.to == to)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 迁移表自身的不变量：没有自环、没有重复边、动作码有名字；终态与出边**正好互补**。
    #[test]
    fn transition_table_is_sane() {
        let mut seen = Vec::new();
        for t in TRANSITIONS {
            assert_ne!(t.from, t.to, "不许自环：{}", t.action);
            assert!(!t.action.is_empty(), "每条边都要有动作码");
            assert!(!seen.contains(&(t.from, t.to)), "重复的边：{t:?}");
            seen.push((t.from, t.to));
        }
        for state in QuestionState::ALL {
            let outgoing = state.next_states().len();
            assert_eq!(
                outgoing == 0,
                state.is_terminal(),
                "「出得去」与「声明为终态」必须正好互补，{} 现在是出边 {outgoing} 条",
                state.as_str()
            );
        }
    }

    #[test]
    fn state_codes_round_trip_and_reject_strangers() {
        for state in QuestionState::ALL {
            assert_eq!(QuestionState::parse(state.as_str()).unwrap(), state);
        }
        let err = QuestionState::parse("nowhere").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_QUESTION_STATE);
    }

    /// 每个态都能从 `pending` 走到——"进得来"这条不变量的**纯逻辑**那一半。
    #[test]
    fn every_state_is_reachable_from_pending() {
        let mut reached = vec![QuestionState::Pending];
        let mut grew = true;
        while grew {
            grew = false;
            for state in reached.clone() {
                for next in state.next_states() {
                    if !reached.contains(&next) {
                        reached.push(next);
                        grew = true;
                    }
                }
            }
        }
        for state in QuestionState::ALL {
            assert!(reached.contains(&state), "{} 从 pending 走不到", state.as_str());
        }
    }
}
