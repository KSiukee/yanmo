//! 模块的使用账：**跑过几次、最后一次是什么时候**。
//!
//! 这不是监控，只回答界面上要显示的两个问题（「用过没有」「多久没用了」）。
//! 账写在 `modules.db` 里（跟模块数据一个库、随库走），但它**不是模块数据**：
//! 模块自己读写不到它，清除模块数据也不清它——那两列说的是「这个模块被启动过几次」，
//! 属于核心的观测，不属于任何一方可以随手改的东西。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use crate::error::Result;
use crate::time::now_millis;

/// 建表语句：核心打开 `modules.db` 时执行（`IF NOT EXISTS`，幂等）。
pub(super) const USAGE_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_usage (
    module_id   TEXT PRIMARY KEY,
    last_run_at INTEGER NOT NULL,
    runs        INTEGER NOT NULL
)";

/// 唤起留痕的表：**一次运行一行**（谁唤起、什么时候、跑了多久、结果）。
///
/// 这就是「链路可见」的地基：今天只有「用户 → 模块」这一种边，
/// 将来的「模块 → 模块」会以 `trigger = chain:<模块 id>` 写进同一张表。
pub(super) const RUN_LOG_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_run_log (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    module_id   TEXT    NOT NULL,
    trigger     TEXT    NOT NULL,
    started_at  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    outcome     TEXT    NOT NULL,
    code        TEXT    NOT NULL DEFAULT ''
)";

/// 每个模块最多留多少条唤起记录（老的删掉；这是观测账，不是审计底稿）。
const RUN_LOG_KEEP: i64 = 200;

/// 一次唤起（链路里的**一条边的一次走过**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunEvent {
    pub module_id: String,
    /// `user`（用户点的）/ `chain:<模块 id>`（将来模块间的唤起）
    pub trigger: String,
    pub started_at: i64,
    pub duration_ms: i64,
    /// `ok` / `error` / `timeout` / `unfinished`
    pub outcome: String,
    pub code: String,
}

/// 一个模块的使用账（没跑过 = 0 次、时刻 0）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleUsage {
    /// 最后一次启动的时刻（毫秒时间戳；0 = 从没跑过）。
    pub last_run_at: i64,
    /// 累计启动次数。
    pub runs: u64,
}

impl ModuleData {
    /// 记一次「跑了」：次数 +1，最后时刻换成现在。
    pub fn note_run(&mut self, module_id: &str) -> Result<()> {
        ensure_id(module_id)?;
        self.conn.execute(
            "INSERT INTO module_usage(module_id, last_run_at, runs) VALUES(?1, ?2, 1)
             ON CONFLICT(module_id) DO UPDATE SET
                 last_run_at = excluded.last_run_at,
                 runs = runs + 1",
            params![module_id, now_millis()],
        )?;
        Ok(())
    }

    /// 记一次**完整的**唤起（谁唤起、跑了多久、结果如何）。
    ///
    /// 与 [`ModuleData::note_run`] 分工：那个只加次数（壳在启动那一刻就记），
    /// 这个记"这一趟的经过"（启动器在模块结束时记）。两者都不清数据、不挡任何人。
    pub fn note_run_result(
        &mut self,
        module_id: &str,
        trigger: &str,
        started_at: i64,
        duration_ms: i64,
        outcome: &str,
        code: &str,
    ) -> Result<()> {
        ensure_id(module_id)?;
        self.conn.execute(
            "INSERT INTO module_run_log(module_id, trigger, started_at, duration_ms, outcome, code)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                module_id,
                trigger,
                started_at,
                duration_ms.max(0),
                outcome,
                code
            ],
        )?;
        // 每个模块只留最近 RUN_LOG_KEEP 条：账要能一直记下去，不能无限长。
        self.conn.execute(
            "DELETE FROM module_run_log
              WHERE module_id = ?1
                AND seq NOT IN (
                    SELECT seq FROM module_run_log WHERE module_id = ?1
                     ORDER BY seq DESC LIMIT ?2
                )",
            params![module_id, RUN_LOG_KEEP],
        )?;
        Ok(())
    }

    /// 最近的唤起（新的在前）。
    pub fn recent_runs(&self, module_id: &str, limit: usize) -> Result<Vec<RunEvent>> {
        ensure_id(module_id)?;
        let limit = limit.clamp(1, 200) as i64;
        let mut stmt = self.conn.prepare(
            "SELECT module_id, trigger, started_at, duration_ms, outcome, code
               FROM module_run_log WHERE module_id = ?1
              ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![module_id, limit], |row| {
                Ok(RunEvent {
                    module_id: row.get(0)?,
                    trigger: row.get(1)?,
                    started_at: row.get(2)?,
                    duration_ms: row.get(3)?,
                    outcome: row.get(4)?,
                    code: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 读这个模块的使用账（没记过就是「没跑过」）。
    pub fn usage(&self, module_id: &str) -> Result<ModuleUsage> {
        ensure_id(module_id)?;
        self.conn
            .query_row(
                "SELECT last_run_at, runs FROM module_usage WHERE module_id = ?1",
                [module_id],
                |row| {
                    Ok(ModuleUsage {
                        last_run_at: row.get(0)?,
                        runs: row.get::<_, i64>(1)?.max(0) as u64,
                    })
                },
            )
            .optional()
            .map(|found| found.unwrap_or(ModuleUsage { last_run_at: 0, runs: 0 }))
            .map_err(Into::into)
    }
}
