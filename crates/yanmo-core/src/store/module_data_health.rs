//! 模块数据健康账：宿主写、核心/界面只读；清除数据时一起清。

use rusqlite::OptionalExtension;
use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use crate::error::Result;

/// 一个模块的数据健康（跨轮；`degraded` 后写入仍被拒，直到清除数据）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleDataHealth {
    pub degraded: bool,
    pub rejected: u64,
}

impl ModuleData {
    pub fn health(&self, module_id: &str) -> Result<ModuleDataHealth> {
        ensure_id(module_id)?;
        let found: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT degraded, rejected FROM module_data_health WHERE module_id = ?1",
                [module_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(found
            .map(|(degraded, rejected)| ModuleDataHealth {
                degraded: degraded != 0,
                rejected: rejected.max(0) as u64,
            })
            .unwrap_or(ModuleDataHealth { degraded: false, rejected: 0 }))
    }

    pub fn clear_health(&mut self, module_id: &str) -> Result<()> {
        ensure_id(module_id)?;
        self.conn.execute(
            "DELETE FROM module_data_health WHERE module_id = ?1",
            [module_id],
        )?;
        Ok(())
    }
}
