//! 一次**投稿**的记录与两条纯判据：冷冻期到没到、这一投会不会撞上一稿多投。
//!
//! 「投过哪几家、还在等回、哪天可改投」是短篇管理真正的价值——
//! 它要的是**历史**（一次投稿一行，见 `submissions` 表），不是作品上的某个字段。
//! 这里只放**数据形状与纯规则**；建表、读写与留痕在 `store::manuscript`（还没接）。
//!
//! # 冷冻期是什么
//!
//! 投出去之后，到"这篇可以再投别家"之间隔着一段**等回音的窗口**：`cooldown_until`，
//! 由作者定（界面填表默认 [`DEFAULT_COOLDOWN_DAYS`] 天）。到点仍没回音就**视为可改投**
//! ——但不改写历史：那条记录还是 `waiting`，只是界面显示「已超期」（编辑后来回了话，
//! 历史不能自己先替他说"没回"）。
//!
//! # 拦是软拦
//!
//! [`verdict`] 给的是**判词**不是判决：不是 [`SubmitVerdict::Clear`] 时，界面把后果说清楚
//! （投给谁、还在等谁、冷冻到哪天），作者确认后**仍然可以投**并留痕——他是主人。

use crate::error::{codes, Error, Result};

/// 填表用的默认冷冻期：90 天（"三个月未回复可自行处理"是文学期刊的常见惯例）。
///
/// 只是**界面预填值**，不进任何判据、不写进库；作者一改就按他的来。
pub const DEFAULT_COOLDOWN_DAYS: i64 = 90;

const DAY_MS: i64 = 86_400_000;

/// 从投稿时刻推出的默认冷冻期截止（作者随时可改）。
pub fn default_cooldown_until(submitted_at: i64) -> i64 {
    submitted_at + DEFAULT_COOLDOWN_DAYS * DAY_MS
}

/// 一次投稿的**结果**。
///
/// `Waiting` 不是"还没填"，而是"投出去了，正在等回音"——冷冻期与一稿多投判据都读它。
/// 「超期未回」**不是一个取值**：它是 `Waiting` + 时间过了截止（[`Submission::is_stale`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionResult {
    /// 等回音（尚未有结果）。
    Waiting,
    /// 过稿。
    Accepted,
    /// 退稿。
    Rejected,
}

/// 进 JSON 就用稳定码本身（理由同 [`super::ManuscriptState`]）。
impl serde::Serialize for SubmissionResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl SubmissionResult {
    /// 全部取值（穷举测试拿它对表）。
    pub const ALL: [SubmissionResult; 3] =
        [SubmissionResult::Waiting, SubmissionResult::Accepted, SubmissionResult::Rejected];

    /// 稳定代码（写库；别改）。
    pub const fn as_str(self) -> &'static str {
        match self {
            SubmissionResult::Waiting => "waiting",
            SubmissionResult::Accepted => "accepted",
            SubmissionResult::Rejected => "rejected",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_submission_result`（不猜着读）。
    pub fn parse(s: &str) -> Result<Self> {
        SubmissionResult::ALL.into_iter().find(|r| r.as_str() == s).ok_or_else(|| {
            Error::invalid_with(codes::UNKNOWN_SUBMISSION_RESULT, [("value", s.to_string())])
        })
    }

    /// 还是"等回音"这一档吗（一稿多投判据只看它）。
    pub const fn is_waiting(self) -> bool {
        matches!(self, SubmissionResult::Waiting)
    }
}

/// 一次投稿。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Submission {
    pub id: i64,
    pub work_id: i64,
    /// 投给谁（平台 / 刊物 / 编辑）——**用户数据，原样存**。
    pub venue: String,
    /// 何时投的（unix 毫秒）。
    pub submitted_at: i64,
    /// 结果。
    pub result: SubmissionResult,
    /// 冷冻期截止（可空＝作者没定期限）。到点未回音即视为可改投。
    pub cooldown_until: Option<i64>,
    /// 备注（用户数据；空串＝没写）。
    pub notes: String,
}

impl Submission {
    /// 还在等这一家的回音。
    pub const fn is_open(&self) -> bool {
        self.result.is_waiting()
    }

    /// 超期：还在等，但冷冻期已经过了——**视为可改投**（历史不改写，界面显示"已超期"）。
    ///
    /// 没定期限（`cooldown_until` 为空）不算超期：作者没说等多久，就不替他假设。
    pub fn is_stale(&self, now: i64) -> bool {
        self.is_open() && self.cooldown_until.is_some_and(|until| now >= until)
    }

    /// 冷冻期未满（`until` 在将来）。
    pub fn is_cooling(&self, now: i64) -> bool {
        self.cooldown_until.is_some_and(|until| now < until)
    }
}

/// 记一次新投稿之前的判词。除 [`SubmitVerdict::Clear`] 外都是**警告**：界面必须把
/// `code()` 对应的那句话与参数摆给作者看，他确认后照样能投（软拦），并留痕。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitVerdict {
    /// 没有拦的理由。
    Clear,
    /// 这篇还在等 `venue` 的回音——再投别家就是一稿多投。
    AwaitingReply { venue: String, submitted_at: i64, cooldown_until: Option<i64> },
    /// `venue` 退过这篇，冷冻期到 `until` 才满。
    VenueCooldown { venue: String, until: i64 },
    /// `venue` 已经投过（`at` 那次）——多半是重复投稿。
    AlreadyTried { venue: String, at: i64 },
}

impl SubmitVerdict {
    /// 稳定代码（界面按它选那句话，核心不给句子）。
    pub const fn code(&self) -> &'static str {
        match self {
            SubmitVerdict::Clear => "clear",
            SubmitVerdict::AwaitingReply { .. } => "awaiting_reply",
            SubmitVerdict::VenueCooldown { .. } => "venue_cooldown",
            SubmitVerdict::AlreadyTried { .. } => "already_tried",
        }
    }

    /// 放不放行（软拦时界面按它决定要不要先弹确认）。
    pub const fn is_clear(&self) -> bool {
        matches!(self, SubmitVerdict::Clear)
    }
}

/// 判一次新投稿（`venue` 投给谁、`now` 现在几点）。规则**按顺序**匹配，第一条命中就返回：
///
/// 1. **还在等别家回音**（`waiting` 且没超期）→ 一稿多投。这条比"这家投过"更该先说，
///    因为它是投稿大忌；超期的旧账（[`Submission::is_stale`]）不再拦——那正是"冷冻期结束可改投"。
/// 2. **这家退过、冷冻期未满** → 同一家不要马上再投。
/// 3. **这家投过** → 提醒重复（含过稿/已发的记录：这篇不该再投这家了）。
/// 4. 其余放行。
///
/// 刊物名先 `trim` 再比，全等即算同一家——不做模糊匹配，猜错了会拦错人，比不拦更糟。
pub fn verdict(history: &[Submission], venue: &str, now: i64) -> SubmitVerdict {
    let venue = venue.trim();
    if let Some(open) = history.iter().find(|s| s.is_open() && !s.is_stale(now)) {
        return SubmitVerdict::AwaitingReply {
            venue: open.venue.clone(),
            submitted_at: open.submitted_at,
            cooldown_until: open.cooldown_until,
        };
    }
    if let Some(s) = history.iter().find(|s| same_venue(s, venue) && !s.is_open() && s.is_cooling(now))
    {
        return SubmitVerdict::VenueCooldown {
            venue: s.venue.clone(),
            until: s.cooldown_until.unwrap_or(0),
        };
    }
    if let Some(s) = history.iter().rev().find(|s| same_venue(s, venue)) {
        return SubmitVerdict::AlreadyTried { venue: s.venue.clone(), at: s.submitted_at };
    }
    SubmitVerdict::Clear
}

fn same_venue(s: &Submission, venue: &str) -> bool {
    s.venue.trim() == venue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_codes_round_trip_and_reject_strangers() {
        for r in SubmissionResult::ALL {
            assert_eq!(SubmissionResult::parse(r.as_str()).unwrap(), r);
        }
        let err = SubmissionResult::parse("pending").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_SUBMISSION_RESULT);
    }

    #[test]
    fn default_cooldown_is_ninety_days() {
        assert_eq!(default_cooldown_until(0), 90 * DAY_MS);
    }
}
