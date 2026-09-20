//! 稿件的**生命周期状态机**：「一篇稿子流转到哪一步了」。
//!
//! 与问题卡状态机（[`super::card_state`]）同形：**纯逻辑**、迁移表是唯一真相源、
//! 「进得来 / 出得去」有机械判据（`tests/manuscript.rs`）。投稿历史（投给谁、何时、
//! 结果、冷冻期）是另一件事，见 [`super::submission`]；落库与留痕见 `store::manuscript`。
//!
//! ```text
//!   草稿 ⇄ 完稿 ⇄ 待投 ──submit──▶ 已投 ──accept──▶ 过稿 ──publish──▶ 已发（终态）
//!                                   │  │
//!                   reject ─────────┘  └──── withdraw ──▶ 待投
//!                      │
//!                      ▼
//!                    被拒 ── submit（改投别家）──▶ 已投
//!                      └── revive / rework ──▶ 待投 / 完稿 / 草稿
//! ```
//!
//! 三条不变量（`tests/manuscript.rs` 里有机械判据）：
//!
//! 1. **进得来**：七个态都能从 `draft` 走到（不留"永远到不了的态"）；
//! 2. **出得去**：只有 [`ManuscriptState::is_terminal`] 声明的终态可以没有出边；
//! 3. **有据可查**：每条迁移落在哪张表、哪个字段、留什么痕，见 `store::manuscript`。
//!
//! # 谁说了算
//!
//! **作者是主人**：草稿 / 完稿 / 待投 之间怎么挪都行（迁移表里那六条边就是为它留的），
//! 机器只拦两种说不通的——**已发出了就回不去**、**进「已投」必须记一次投稿**
//! （`submit` 这条边在存储层要求同时写一条投稿记录）。「完稿」**不是拦投的门槛**：
//! 短篇常常写完就投，从草稿直接进「已投」也允许（补记）。

use crate::error::{codes, Error, Result};

/// 一篇稿件的流转状态。稳定代码写进库（`works.status`）——**别改**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ManuscriptState {
    /// 草稿：还在写。
    Draft,
    /// 完稿：初稿写完，还没准备投（也用来放"先搁着"）。
    Finished,
    /// 待投：这篇可以投了。
    Ready,
    /// 已投：投出去了，在等回音（冷冻期中）。
    Submitted,
    /// 过稿：编辑收了。
    Accepted,
    /// 被拒：退稿。**不是终点**——冷冻期结束可改投别家（`revive`），也可回炉重写。
    Rejected,
    /// 已发：见刊 / 上线。**终态**（不再有出边）。
    Published,
}

/// 进 JSON 就用**稳定码本身**，理由与 [`super::QuestionState`] 相同：
/// 靠"改蛇形"迟早分家，而且分家时不报错，只是界面静默对不上。
impl serde::Serialize for ManuscriptState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl ManuscriptState {
    /// 全部取值——**顺序即看板的列顺序**（界面按它排，穷举测试拿它对表）。
    pub const ALL: [ManuscriptState; 7] = [
        ManuscriptState::Draft,
        ManuscriptState::Finished,
        ManuscriptState::Ready,
        ManuscriptState::Submitted,
        ManuscriptState::Accepted,
        ManuscriptState::Rejected,
        ManuscriptState::Published,
    ];

    /// 稳定代码（写库 / 进 JSON；别改）。
    pub const fn as_str(self) -> &'static str {
        match self {
            ManuscriptState::Draft => "draft",
            ManuscriptState::Finished => "finished",
            ManuscriptState::Ready => "ready",
            ManuscriptState::Submitted => "submitted",
            ManuscriptState::Accepted => "accepted",
            ManuscriptState::Rejected => "rejected",
            ManuscriptState::Published => "published",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_manuscript_state`（不猜着读）。
    pub fn parse(s: &str) -> Result<Self> {
        ManuscriptState::ALL.into_iter().find(|state| state.as_str() == s).ok_or_else(|| {
            Error::invalid_with(codes::UNKNOWN_MANUSCRIPT_STATE, [("value", s.to_string())])
        })
    }

    /// 终态：没有出边是**有意的**，不是漏了迁移。只有 `published` 是终态；
    /// `rejected` 刻意**不算**——"被拒 → 冷冻期结束 → 可改投"正是这条状态机存在的理由。
    pub const fn is_terminal(self) -> bool {
        matches!(self, ManuscriptState::Published)
    }

    /// 默认状态：新稿子都是草稿。
    pub const fn initial() -> Self {
        ManuscriptState::Draft
    }

    /// 从这里出发的合法迁移。
    pub fn next_states(self) -> Vec<ManuscriptState> {
        MANUSCRIPT_TRANSITIONS.iter().filter(|t| t.from == self).map(|t| t.to).collect()
    }
}

/// 迁移表里的简写——只在本文件用，省得每行都写全名。
type State = ManuscriptState;
type Edge = ManuscriptTransition;

/// 一次合法迁移：从哪到哪、动作码叫什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManuscriptTransition {
    pub from: ManuscriptState,
    pub to: ManuscriptState,
    /// **稳定动作码**——它就是 op-log 里的 `op`（"谁触发"另有一个 `trigger` 参数）。
    ///
    /// 同一个用户意图可以出现在几条边上（`ready` / `reopen` / `submit` / `rework`）：
    /// 动作码说的是**作者做了什么**，`from` / `to` 另记在 payload 里。
    pub action: &'static str,
}

/// 完整迁移表——**唯一真相源**：允许怎么跳、跳的动作叫什么，都只写在这里。
/// 每条边上方那句注释说的是"这条边为什么允许"（纯注释：不当界面文案，也不进库）。
pub const MANUSCRIPT_TRANSITIONS: &[Edge] = &[
    // 写完初稿；没完稿就直接标成可投——短篇常常写完即投，不让作者先去点一下"完稿"
    Edge { from: State::Draft, to: State::Finished, action: "finish" },
    Edge { from: State::Draft, to: State::Ready, action: "ready" },
    // 退回草稿接着改；完稿了，可以投
    Edge { from: State::Finished, to: State::Draft, action: "reopen" },
    Edge { from: State::Finished, to: State::Ready, action: "ready" },
    // 又想改了；先别投，放回完稿（含"再打磨一轮"）
    Edge { from: State::Ready, to: State::Draft, action: "reopen" },
    Edge { from: State::Ready, to: State::Finished, action: "hold" },
    // 记一次投稿：还没投出去的三个阶段都能直接补记（写完就投、忘了先标状态）
    Edge { from: State::Draft, to: State::Submitted, action: "submit" },
    Edge { from: State::Finished, to: State::Submitted, action: "submit" },
    Edge { from: State::Ready, to: State::Submitted, action: "submit" },
    // 被拒之后改投别家，同样是"记一次投稿"
    Edge { from: State::Rejected, to: State::Submitted, action: "submit" },
    // 等回音的结果：过稿 / 退稿 / 自己把稿子收回来（撤稿、等太久不等了）
    Edge { from: State::Submitted, to: State::Accepted, action: "accept" },
    Edge { from: State::Submitted, to: State::Rejected, action: "reject" },
    Edge { from: State::Submitted, to: State::Ready, action: "withdraw" },
    // 冷冻期结束改投别家（这条重出路径正是本状态机存在的理由）；退稿后回炉重写
    Edge { from: State::Rejected, to: State::Ready, action: "revive" },
    Edge { from: State::Rejected, to: State::Finished, action: "rework" },
    Edge { from: State::Rejected, to: State::Draft, action: "rework" },
    // 过稿之后见刊 / 上线——到这里就没有出边了
    Edge { from: State::Accepted, to: State::Published, action: "publish" },
];

/// 查一条边；没有就是非法迁移（存储层据此报 `manuscript.illegal_transition`，一个字节都不写）。
pub fn manuscript_transition(from: State, to: State) -> Option<&'static ManuscriptTransition> {
    MANUSCRIPT_TRANSITIONS.iter().find(|t| t.from == from && t.to == to)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 迁移表自身的不变量：没有自环、没有重复边、动作码有名字；终态与出边**正好互补**。
    #[test]
    fn transition_table_is_sane() {
        let mut seen = Vec::new();
        for t in MANUSCRIPT_TRANSITIONS {
            assert_ne!(t.from, t.to, "不许自环：{}", t.action);
            assert!(!t.action.is_empty(), "每条边都要有动作码");
            assert!(!seen.contains(&(t.from, t.to)), "重复的边：{t:?}");
            seen.push((t.from, t.to));
        }
        for state in ManuscriptState::ALL {
            let outgoing = state.next_states().len();
            assert_eq!(
                outgoing == 0,
                state.is_terminal(),
                "「出得去」与「声明为终态」必须互补：{}",
                state.as_str()
            );
        }
    }

    #[test]
    fn state_codes_round_trip_and_reject_strangers() {
        for state in ManuscriptState::ALL {
            assert_eq!(ManuscriptState::parse(state.as_str()).unwrap(), state);
        }
        let err = ManuscriptState::parse("archived").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_MANUSCRIPT_STATE);
    }
}
