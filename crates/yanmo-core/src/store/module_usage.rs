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
