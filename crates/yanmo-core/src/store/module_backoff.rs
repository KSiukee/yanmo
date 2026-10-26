//! 失败之后**别马上再来一次**：按连续失败次数退避（只挡"自动唤起"，不挡作者手点）。
//!
//! # 为什么只挡自动
//!
//! 作者点"运行"就是他的决定——失败了他自己看得见，再点一次是他的自由；退避去挡
//! 手动点击只会让人困惑。真正需要退避的是**模块间唤起**：一个坏掉的模块被链条反复
//! 叫起来，就是无声的唤醒风暴（扇出上限只挡"总次数"，挡不住"同一个坏模块被叫 N 次"）。
//!
//! # 规则
//!
//! 连续失败 `n` 次 → 等 `10s * 2^(n-1)`，上限 **5 分钟**；成功一次立刻清零。
//! 状态跟库走（`module_backoff` 表），重启不清——坏模块不该靠重启洗白。

use rusqlite::params;
use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use crate::error::Result;
use crate::time::now_millis;

/// 退避参数（可按模块调；不设就用默认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RetryPolicy {
    /// 第一次失败后等多久
    pub base_ms: i64,
    /// 封顶
    pub max_ms: i64,
    /// 关掉 = 不挡自动唤起（作者自己承担唤醒风暴）
    pub enabled: bool,
}

impl RetryPolicy {
    pub fn default_policy() -> Self {
        Self { base_ms: BASE_MS, max_ms: MAX_MS, enabled: true }
    }

    /// 校验：1 秒 ~ 10 分钟起步、封顶不小于起步、封顶 ≤ 1 小时。
    pub fn validate(&self) -> crate::error::Result<()> {
        let ok = self.base_ms >= 1_000
            && self.base_ms <= 10 * 60 * 1000
            && self.max_ms >= self.base_ms
            && self.max_ms <= 60 * 60 * 1000;
        if ok {
            Ok(())
        } else {
            Err(crate::error::Error::invalid_with(
                crate::error::codes::MODULE_BACKOFF_INVALID,
                [
                    ("base_ms", self.base_ms.to_string()),
                    ("max_ms", self.max_ms.to_string()),
                ],
            ))
        }
    }
}

pub(super) const RETRY_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_retry (
    module_id TEXT PRIMARY KEY,
    base_ms   INTEGER NOT NULL,
    max_ms    INTEGER NOT NULL,
    enabled   INTEGER NOT NULL
)";

pub(super) const BACKOFF_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_backoff (
    module_id   TEXT PRIMARY KEY,
    failures    INTEGER NOT NULL,
    next_at     INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
)";

/// 一次退避读数（给界面显示"还要等多久"）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backoff {
    pub module_id: String,
    pub failures: i64,
    /// 在此时刻之前，自动唤起会被拒（毫秒时间戳）
    pub next_at: i64,
}

/// 第一次退避等多久，以及上限。
const BASE_MS: i64 = 10_000;
const MAX_MS: i64 = 5 * 60 * 1000;

impl ModuleData {
    /// 记一次结果：成功清零；失败 +1 并按次数算下一次可用的时刻。
    pub fn note_backoff(&mut self, module_id: &str, ok: bool) -> Result<()> {
        ensure_id(module_id)?;
        let now = now_millis();
        if ok {
            self.conn.execute("DELETE FROM module_backoff WHERE module_id = ?1", [module_id])?;
            return Ok(());
        }
        let failures: i64 = self
            .conn
            .query_row(
                "SELECT failures FROM module_backoff WHERE module_id = ?1",
                [module_id],
                |row| row.get(0),
            )
            .unwrap_or(0)
            + 1;
        // 2^(n-1) 放大，封顶"这个模块的封顶"；位数不越界（n 很大时也安全）
        let policy = self.retry_policy(module_id)?;
        let shift = (failures - 1).clamp(0, 20) as u32;
        let wait = policy.base_ms.saturating_mul(1_i64 << shift).min(policy.max_ms);
        self.conn.execute(
            "INSERT INTO module_backoff(module_id, failures, next_at, updated_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(module_id) DO UPDATE SET
                 failures = excluded.failures,
                 next_at = excluded.next_at,
                 updated_at = excluded.updated_at",
            params![module_id, failures, now + wait, now],
        )?;
        Ok(())
    }

    /// 这个模块的退避参数（没设过 = 默认）。
    pub fn retry_policy(&self, module_id: &str) -> Result<RetryPolicy> {
        ensure_id(module_id)?;
        let row = self
            .conn
            .query_row(
                "SELECT base_ms, max_ms, enabled FROM module_retry WHERE module_id = ?1",
                [module_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .ok();
        Ok(row
            .map(|(base_ms, max_ms, enabled)| RetryPolicy {
                base_ms,
                max_ms,
                enabled: enabled != 0,
            })
            .unwrap_or_else(RetryPolicy::default_policy))
    }

    /// 改这个模块的退避参数（先校验；关掉 = 不挡自动唤起）。
    pub fn set_retry_policy(&mut self, module_id: &str, policy: &RetryPolicy) -> Result<()> {
        ensure_id(module_id)?;
        policy.validate()?;
        self.conn.execute(
            "INSERT INTO module_retry(module_id, base_ms, max_ms, enabled)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(module_id) DO UPDATE SET
                 base_ms = excluded.base_ms,
                 max_ms = excluded.max_ms,
                 enabled = excluded.enabled",
            params![module_id, policy.base_ms, policy.max_ms, i64::from(policy.enabled)],
        )?;
        Ok(())
    }

    /// 回到默认参数（删掉这一行，不是写一份"和默认一样"的副本）。
    pub fn clear_retry_policy(&mut self, module_id: &str) -> Result<()> {
        ensure_id(module_id)?;
        self.conn.execute("DELETE FROM module_retry WHERE module_id = ?1", [module_id])?;
        Ok(())
    }

    /// 现在能不能自动唤起它（`true` = 可以）。没记录 = 可以。
    pub fn backoff_allows(&self, module_id: &str) -> Result<bool> {
        ensure_id(module_id)?;
        if !self.retry_policy(module_id)?.enabled {
            return Ok(true);
        }
        let next_at: Option<i64> = self
            .conn
            .query_row(
                "SELECT next_at FROM module_backoff WHERE module_id = ?1",
                [module_id],
                |row| row.get(0),
            )
            .ok();
        Ok(next_at.map(|at| now_millis() >= at).unwrap_or(true))
    }

    /// 读一个模块的退避账（界面显示"还要等多久"）。
    pub fn backoff(&self, module_id: &str) -> Result<Option<Backoff>> {
        ensure_id(module_id)?;
        let row = self
            .conn
            .query_row(
                "SELECT failures, next_at FROM module_backoff WHERE module_id = ?1",
                [module_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        Ok(row.map(|(failures, next_at)| Backoff {
            module_id: module_id.to_string(),
            failures,
            next_at,
        }))
    }
}
