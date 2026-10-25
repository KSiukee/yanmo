//! 读取的**渐进放行**：新装的第三方模块一次运行只许读一篇文章；被限量拦下就记一笔，
//! 第二、第三次由作者决定要不要放宽——**放不放宽是作者的事，系统只负责把事实摆出来**。
//!
//! # 三档
//!
//! | 档 | 一次运行能读几篇正文 | 怎么到这一档 |
//! |---|---|---|
//! | 0 | 1 篇 | 新装默认（未认证的第三方模块） |
//! | 1 | 2 篇 | 作者在「第二次」被问到时选择"给更多" |
//! | 2 | 不限 | 作者在「第三次」被问到时选择"给更多"（此后再不干预） |
//!
//! 认证模块与官方模块（`app.yanmo.` 前缀）**不走这道闸**——认证的价值就在这里
//! （换更少摩擦），但留痕与告警照旧。
//!
//! # 谁记、记在哪
//!
//! 账落在 `modules.db`（跟模块的其他账一个库：结构版本 / 健康 / 隔离 / 使用）：
//! 一行一个模块，`stage` 是当前档、`refusals` 是"被限量拦下过几次运行"、
//! `answered` 是作者最后一次作答时的 `refusals`——**它让同一个数不会问两遍**。
//!
//! # 为什么按"运行"计数而不是按次
//!
//! 一个模块一次运行里连读五章，如果按次计数，一跑就直接跳到"第三次"，
//! 作者根本没机会逐级回答。按"这次运行超了额度"计一笔，才和用户要的节奏对上：
//! 第一次→告知，第二次→问一句，第三次→终极确认。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use crate::error::Result;
use crate::time::now_millis;

/// 建表语句：核心打开 `modules.db` 时执行（`IF NOT EXISTS`，幂等）。
pub(super) const CONSENT_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_read_consent (
    module_id  TEXT PRIMARY KEY,
    stage      INTEGER NOT NULL,
    refusals   INTEGER NOT NULL,
    answered   INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)";

/// 档位上限（`2` 就是"不再干预"）。
const STAGE_UNLIMITED: u8 = 2;

/// 一个模块的读取放行状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadConsent {
    /// 0 = 一篇文章 / 1 = 两篇 / 2 = 不再限制。
    pub stage: u8,
    /// 被限量拦下过几次运行。
    pub refusals: u64,
    /// 作者最后一次作答时的 `refusals`（0 = 从没答过）。
    pub answered: u64,
}

impl Default for ReadConsent {
    fn default() -> Self {
        Self { stage: 0, refusals: 0, answered: 0 }
    }
}

/// 一次运行能读几篇正文（`None` = 不限）。
pub fn body_budget(stage: u8) -> Option<u64> {
    match stage {
        0 => Some(1),
        1 => Some(2),
        _ => None,
    }
}

/// 有待作者回答/确认的事吗？
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadAsk {
    /// `first`（只告知：只给了一篇）/ `second`（问要不要继续放宽到两篇）/
    /// `ultimate`（第三次：确认后不再干预）。
    pub kind: &'static str,
    /// 当前是第几次（作者看到的就是这个数）。
    pub refusals: u64,
}

/// 该不该问作者：**只看状态，不碰数据库**（纯逻辑，单测盯它）。
///
/// - 已经不限制了 → 不问；
/// - 这个数已经答过 → 不重复问；
/// - 第 1 次 → 只告知（`first`）；第 2 次 → 问（`second`）；第 3 次及以后 → 终极确认。
pub fn pending_ask(consent: &ReadConsent) -> Option<ReadAsk> {
    if consent.stage >= STAGE_UNLIMITED {
        return None;
    }
    if consent.refusals == 0 || consent.answered >= consent.refusals {
        return None;
    }
    let kind = match consent.refusals {
        0..=1 => "first",
        2 => "second",
        _ => "ultimate",
    };
    Some(ReadAsk { kind, refusals: consent.refusals })
}

/// 作者的一次作答。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadAnswer {
    /// 知道了（只对「第一次告知」有意义）。
    Acknowledge,
    /// 给更多（第二档 → 两篇；第三档 → 不再限制）。
    AllowMore,
    /// 保持现在的限制。
    KeepLimited,
}

impl ReadAnswer {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "acknowledge" => Some(ReadAnswer::Acknowledge),
            "allow_more" => Some(ReadAnswer::AllowMore),
            "keep_limited" => Some(ReadAnswer::KeepLimited),
            _ => None,
        }
    }
}

impl ModuleData {
    /// 读一个模块的放行状态（没记过 = 新装：一篇文章）。
    pub fn read_consent(&self, module_id: &str) -> Result<ReadConsent> {
        ensure_id(module_id)?;
        self.conn
            .query_row(
                "SELECT stage, refusals, answered FROM module_read_consent WHERE module_id = ?1",
                [module_id],
                |row| {
                    Ok(ReadConsent {
                        stage: row.get::<_, i64>(0)?.clamp(0, STAGE_UNLIMITED as i64) as u8,
                        refusals: row.get::<_, i64>(1)?.max(0) as u64,
                        answered: row.get::<_, i64>(2)?.max(0) as u64,
                    })
                },
            )
            .optional()
            .map(|found| found.unwrap_or_default())
            .map_err(Into::into)
    }

    /// 记一笔"这次运行超了额度"（同一运行里只该记一次，由调用方保证）。
    pub fn note_read_refusal(&mut self, module_id: &str) -> Result<ReadConsent> {
        ensure_id(module_id)?;
        let current = self.read_consent(module_id)?;
        let next = ReadConsent { refusals: current.refusals + 1, ..current };
        self.save_consent(module_id, &next)?;
        Ok(next)
    }

    /// 作者作答：放宽一档 / 保持限制 / 知道了。
    ///
    /// 三种作答都把 `answered` 推到当前 `refusals`——**同一个数不会问第二遍**；
    /// 只有 `AllowMore` 会抬 `stage`。
    pub fn answer_read_ask(&mut self, module_id: &str, answer: ReadAnswer) -> Result<ReadConsent> {
        ensure_id(module_id)?;
        let current = self.read_consent(module_id)?;
        let stage = match answer {
            ReadAnswer::AllowMore => (current.stage + 1).min(STAGE_UNLIMITED),
            _ => current.stage,
        };
        let next = ReadConsent { stage, answered: current.refusals.max(current.answered), ..current };
        self.save_consent(module_id, &next)?;
        Ok(next)
    }

    /// 所有**等着作者回答**的模块（面板上那一条条就是它）。
    pub fn read_asks(&self) -> Result<Vec<(String, ReadAsk)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT module_id, stage, refusals, answered FROM module_read_consent ORDER BY module_id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ReadConsent {
                        stage: row.get::<_, i64>(1)?.clamp(0, STAGE_UNLIMITED as i64) as u8,
                        refusals: row.get::<_, i64>(2)?.max(0) as u64,
                        answered: row.get::<_, i64>(3)?.max(0) as u64,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(id, consent)| pending_ask(&consent).map(|ask| (id, ask))).collect())
    }

    fn save_consent(&mut self, module_id: &str, consent: &ReadConsent) -> Result<()> {
        self.conn.execute(
            "INSERT INTO module_read_consent(module_id, stage, refusals, answered, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(module_id) DO UPDATE SET
                 stage = excluded.stage,
                 refusals = excluded.refusals,
                 answered = excluded.answered,
                 updated_at = excluded.updated_at",
            params![
                module_id,
                consent.stage as i64,
                consent.refusals as i64,
                consent.answered as i64,
                now_millis()
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_matches_the_three_stages() {
        assert_eq!(body_budget(0), Some(1));
        assert_eq!(body_budget(1), Some(2));
        assert_eq!(body_budget(2), None);
    }

    #[test]
    fn the_first_refusal_is_only_a_notice() {
        let ask = pending_ask(&ReadConsent { stage: 0, refusals: 1, answered: 0 }).unwrap();
        assert_eq!(ask.kind, "first");
        assert_eq!(ask.refusals, 1);
    }

    #[test]
    fn the_second_and_third_refusals_ask_a_question() {
        assert_eq!(
            pending_ask(&ReadConsent { stage: 0, refusals: 2, answered: 1 }).unwrap().kind,
            "second"
        );
        assert_eq!(
            pending_ask(&ReadConsent { stage: 1, refusals: 3, answered: 2 }).unwrap().kind,
            "ultimate"
        );
        // 第四次及以后还是终极确认（"这是第 N 次"由界面说）
        assert_eq!(
            pending_ask(&ReadConsent { stage: 1, refusals: 7, answered: 3 }).unwrap().kind,
            "ultimate"
        );
    }

    #[test]
    fn answered_questions_are_not_asked_twice() {
        assert!(pending_ask(&ReadConsent { stage: 1, refusals: 2, answered: 2 }).is_none());
        assert!(pending_ask(&ReadConsent { stage: 2, refusals: 9, answered: 3 }).is_none());
        assert!(pending_ask(&ReadConsent::default()).is_none(), "从没被拦过就不问");
    }

    #[test]
    fn answers_parse_by_stable_code() {
        assert_eq!(ReadAnswer::parse("allow_more"), Some(ReadAnswer::AllowMore));
        assert_eq!(ReadAnswer::parse("keep_limited"), Some(ReadAnswer::KeepLimited));
        assert_eq!(ReadAnswer::parse("acknowledge"), Some(ReadAnswer::Acknowledge));
        assert_eq!(ReadAnswer::parse("whatever"), None);
    }
}
