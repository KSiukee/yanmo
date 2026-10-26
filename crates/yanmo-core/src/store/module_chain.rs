//! 唤起的**边**：哪条边被作者断了，**断电即断，重启还在**。
//!
//! # 为什么先有"断链"再有"链路"
//!
//! 模块间唤起（A 叫 B）的调用 ABI 还没落地；但"作者能掐断 A→B"这件事可以
//! 先立起来——它是**承诺**：今天断了，将来那条 ABI 通了也绕不过。断链表存在
//! `modules.db` 里（跟着库走），**不随模块卸载/数据清空消失**。
//!
//! # 断的是什么
//!
//! 断的是**这一条边**（`from → to`），不是把 B 停用：B 自己点还是能跑，
//! 只是"由 A 唤起 B"这条路不通了。自环（A→A）没有意义，直接拒。

use rusqlite::params;
use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use crate::error::Result;
use crate::time::now_millis;

/// 建表语句（幂等；与 `module_usage` / `module_run_log` 同一个库）。
pub(super) const CHAIN_CUT_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS module_chain_cut (
    from_module TEXT    NOT NULL,
    to_module   TEXT    NOT NULL,
    cut_at      INTEGER NOT NULL,
    PRIMARY KEY (from_module, to_module)
)";

/// 一条被断开的边。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainCut {
    pub from: String,
    pub to: String,
    pub cut_at: i64,
}

impl ModuleData {
    /// 断掉一条边（重复断 = 幂等，保留第一次的时刻）。
    pub fn cut_edge(&mut self, from: &str, to: &str) -> Result<()> {
        ensure_id(from)?;
        ensure_id(to)?;
        if from == to {
            return Err(crate::error::Error::invalid_with(
                crate::error::codes::MODULE_CHAIN_SELF,
                [("module_id", from.to_string())],
            ));
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO module_chain_cut(from_module, to_module, cut_at)
             VALUES(?1, ?2, ?3)",
            params![from, to, now_millis()],
        )?;
        Ok(())
    }

    /// 接回一条边；返回**这次是否真的接了**（原来就没断 = `false`）。
    pub fn restore_edge(&mut self, from: &str, to: &str) -> Result<bool> {
        ensure_id(from)?;
        ensure_id(to)?;
        let removed = self.conn.execute(
            "DELETE FROM module_chain_cut WHERE from_module = ?1 AND to_module = ?2",
            params![from, to],
        )?;
        Ok(removed > 0)
    }

    /// 这条边断没断（将来模块间调用 ABI 的**唯一判定入口**）。
    pub fn is_cut(&self, from: &str, to: &str) -> Result<bool> {
        ensure_id(from)?;
        ensure_id(to)?;
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT cut_at FROM module_chain_cut WHERE from_module = ?1 AND to_module = ?2",
                params![from, to],
                |row| row.get(0),
            )
            .ok();
        Ok(found.is_some())
    }

    /// 全部断开的边（界面按这个列）。
    pub fn cuts(&self) -> Result<Vec<ChainCut>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_module, to_module, cut_at FROM module_chain_cut
              ORDER BY cut_at DESC, from_module, to_module",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ChainCut { from: row.get(0)?, to: row.get(1)?, cut_at: row.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
