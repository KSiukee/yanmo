//! 模块数据隔离：**把出事的那一格关起来，现场原样留着**。
//!
//! # 隔离不是删除
//!
//! 隔离只做一件事：这个模块**不能再读、写、改自己的数据**（KV 与受控迁移都拒），
//! 但数据一个字节都不动——导出、体检、清除照旧可用。这样坏数据既不会继续长大，
//! 也不会连累别的模块，作者还有机会把它导出来看看到底怎么了。
//!
//! # 为什么是显式动作
//!
//! 解除隔离**只认作者点按钮**，不做「自动恢复」。理由与体检本身同一条：核心只给事实
//! （体检说了什么），判断「这模块还要不要用」是作者的事——自动放开等于把结论偷偷改掉。
//!
//! 隔离状态落 `modules.db` 的 `module_data_isolation`（一行 = 一个被隔离的模块）：
//! 跨轮有效，核心与宿主各自读同一张表，谁都不能绕过。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 隔离原因最多留多少字（界面一句话就够，长了只是把日志搬进库里）。
const MAX_REASON_CHARS: usize = 200;

/// 建表语句：核心与宿主打开时都执行一次（`IF NOT EXISTS`，幂等）。
pub(super) const ISOLATION_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_data_isolation (
    module_id   TEXT PRIMARY KEY,
    reason      TEXT NOT NULL,
    isolated_at INTEGER NOT NULL
)";

/// 一个模块的隔离状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleIsolation {
    /// 为什么隔离（作者在界面上写的一句话；可以是空串）。
    pub reason: String,
    /// 隔离时刻（毫秒时间戳）。
    pub isolated_at: i64,
}

impl ModuleData {
    /// 这个模块被隔离了吗（没隔离返回 `None`）。
    pub fn isolation(&self, module_id: &str) -> Result<Option<ModuleIsolation>> {
        ensure_id(module_id)?;
        self.conn
            .query_row(
                "SELECT reason, isolated_at FROM module_data_isolation WHERE module_id = ?1",
                [module_id],
                |row| {
                    Ok(ModuleIsolation { reason: row.get(0)?, isolated_at: row.get(1)? })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// 隔离一个模块（已在隔离中则更新原因与时刻——重复点按钮不报错）。
    pub fn isolate(&mut self, module_id: &str, reason: &str) -> Result<ModuleIsolation> {
        ensure_id(module_id)?;
        let reason: String = reason.trim().chars().take(MAX_REASON_CHARS).collect();
        let isolated_at = now_millis();
        self.conn.execute(
            "INSERT INTO module_data_isolation(module_id, reason, isolated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(module_id) DO UPDATE SET
                 reason = excluded.reason,
                 isolated_at = excluded.isolated_at",
            params![module_id, reason, isolated_at],
        )?;
        Ok(ModuleIsolation { reason, isolated_at })
    }

    /// 解除隔离（返回「本来是不是隔离着」）。
    pub fn release_isolation(&mut self, module_id: &str) -> Result<bool> {
        ensure_id(module_id)?;
        let removed = self.conn.execute(
            "DELETE FROM module_data_isolation WHERE module_id = ?1",
            [module_id],
        )?;
        Ok(removed > 0)
    }

    /// 写入口的公共闸：隔离了就拒（**读也拒**——半开的门比关着更危险）。
    pub(super) fn ensure_not_isolated(&self, module_id: &str) -> Result<()> {
        if self.isolation(module_id)?.is_some() {
            return Err(Error::invalid_with(
                codes::MODULE_DATA_ISOLATED,
                [("module_id", module_id.to_string())],
            ));
        }
        Ok(())
    }
}
