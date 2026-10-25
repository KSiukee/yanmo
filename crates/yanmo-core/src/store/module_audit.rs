//! 模块数据体检：把**已经存在的事实**翻成一句结论（只读，一个字节都不改）。
//!
//! # 体检看什么
//!
//! - **库文件本身**：`PRAGMA quick_check` 的三态结论（好的 / 有问题 / **这次没查成**）——
//!   三态分开是既有口径（见 [`crate::db::Integrity`]），"没查成"绝不说成"坏了"；
//! - **表的结构**：KV 表在不在、`key` / `value` / `updated_at` 三列缺不缺；
//! - **账与数据对不对得上**：记了 schema 版本却一张表都没有 = 数据丢了；
//! - **命名空间**：本模块前缀下有没有后缀不合规的表（写了但核心不认）；
//! - **规模**：表数有没有超限；写守卫有没有把它降级；
//! - **库里的野表**：`modules.db` 中谁也认不出归属的表有几张。
//!
//! # 只给事实，不给分数
//!
//! 结论只有三档：`ok`（没发现问题）/ `attention`（要看一眼，但不是数据坏了）/
//! `damaged`（数据或库文件确实有问题）。**不做 0~100 分的健康评分**——那种分数
//! 既不可核对，也会变成"看着还行"的免责符号。

use rusqlite::Connection;
use serde::Serialize;

use super::module_data::{ensure_id, quote, ModuleData};
use super::module_namespace::{classify_table, TableOwner};
use crate::db::Integrity;
use crate::error::Result;

/// 体检里的一条事实（`code` 稳定，界面按它取句子；`detail` 是技术细节）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleIssue {
    /// 稳定代码：`corrupt` / `not_checked` / `kv_shape` / `lost_tables` /
    /// `junk_table` / `table_limit` / `degraded`。
    pub code: &'static str,
    /// 技术细节（表名、列名、SQLite 原话）——给人核对，不当界面句子用。
    pub detail: String,
}

impl ModuleIssue {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

/// 一个模块的数据体检报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleAudit {
    /// 模块 id。
    pub module_id: String,
    /// 库文件完整性：`clean` / `problem` / `not_checked`。
    pub integrity: &'static str,
    /// SQLite 的原话（留档与核对用）。
    pub integrity_raw: String,
    /// 这个模块的表数。
    pub tables: usize,
    /// 这个模块的行数合计。
    pub rows: i64,
    /// 整个 `modules.db` 里谁也认不出的表有几张（不属于这个模块，但同一口锅里）。
    pub stray_tables: usize,
    /// 这个模块现在是不是被隔离着。
    pub isolated: bool,
    /// 隔离原因（没隔离就是空串）。
    pub isolation_reason: String,
    /// 写守卫有没有把它降级为只读。
    pub degraded: bool,
    /// 发现的事实（空的 = 没发现问题）。
    pub issues: Vec<ModuleIssue>,
    /// 一句结论：`ok` / `attention` / `damaged`。
    pub verdict: &'static str,
}

impl ModuleData {
    /// 体检一个模块（只读；隔离中的模块照样能体检——要先看清再决定放不放）。
    pub fn audit(&self, module_id: &str) -> Result<ModuleAudit> {
        ensure_id(module_id)?;
        let stats = self.stats(module_id)?;
        let tables = self.tables_of(module_id)?;
        let health = self.health(module_id)?;
        let isolation = self.isolation(module_id)?;
        let integrity = crate::db::integrity(&self.conn)?;

        let mut issues = Vec::new();
        match &integrity {
            Integrity::Clean => {}
            Integrity::Problem(text) => issues.push(ModuleIssue::new("corrupt", text.clone())),
            Integrity::NotChecked(text) => issues.push(ModuleIssue::new("not_checked", text.clone())),
        }
        if let Some(gap) = self.kv_shape_gap(module_id) {
            issues.push(ModuleIssue::new("kv_shape", gap));
        }
        if self.schema_version(module_id)?.is_some() && tables.is_empty() {
            issues.push(ModuleIssue::new("lost_tables", "schema_version recorded, no table"));
        }
        let junk = self.junk_tables(module_id)?;
        if !junk.is_empty() {
            issues.push(ModuleIssue::new("junk_table", junk.join(", ")));
        }
        if tables.len() > self.limits.max_tables {
            issues.push(ModuleIssue::new(
                "table_limit",
                format!("{} > {}", tables.len(), self.limits.max_tables),
            ));
        }
        if health.degraded {
            issues.push(ModuleIssue::new("degraded", format!("rejected {}", health.rejected)));
        }

        let verdict = verdict_of(&issues);
        Ok(ModuleAudit {
            module_id: module_id.to_string(),
            integrity: integrity.as_str(),
            integrity_raw: integrity.raw().to_string(),
            tables: stats.tables,
            rows: stats.rows,
            stray_tables: self.stray_table_count()?,
            isolated: isolation.is_some(),
            isolation_reason: isolation.map(|state| state.reason).unwrap_or_default(),
            degraded: health.degraded,
            issues,
            verdict,
        })
    }

    /// 本模块前缀下、后缀不合规的表（核心不认，但它们确实占着地方）。
    fn junk_tables(&self, module_id: &str) -> Result<Vec<String>> {
        let prefix = Self::table_prefix(module_id);
        Ok(self
            .all_tables()?
            .into_iter()
            .filter(|table| table.starts_with(&prefix))
            .filter(|table| !matches!(classify_table(table), TableOwner::Module { .. }))
            .collect())
    }

    /// KV 表存在但缺列 → 返回缺了哪几列（表本来就不存在不算毛病：还没写过数据）。
    fn kv_shape_gap(&self, module_id: &str) -> Option<String> {
        let table = format!("{}kv", Self::table_prefix(module_id));
        let columns = columns_of(&self.conn, &table).ok()?;
        if columns.is_empty() {
            return None;
        }
        let missing: Vec<&str> = ["key", "value", "updated_at"]
            .into_iter()
            .filter(|column| !columns.iter().any(|found| found == column))
            .collect();
        if missing.is_empty() {
            None
        } else {
            Some(format!("{}: missing {}", table, missing.join(",")))
        }
    }

    /// 整个库里谁也认不出归属的表有几张。
    fn stray_table_count(&self) -> Result<usize> {
        Ok(self
            .all_tables()?
            .into_iter()
            .filter(|table| {
                matches!(
                    classify_table(table),
                    TableOwner::Unattributed | TableOwner::Foreign
                )
            })
            .count())
    }

    fn all_tables(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(names)
    }
}

/// 结论：有硬伤 = `damaged`；只有需要看一眼的事 = `attention`；一条都没有 = `ok`。
fn verdict_of(issues: &[ModuleIssue]) -> &'static str {
    const DAMAGE: [&str; 4] = ["corrupt", "kv_shape", "lost_tables", "junk_table"];
    if issues.iter().any(|issue| DAMAGE.contains(&issue.code)) {
        "damaged"
    } else if issues.is_empty() {
        "ok"
    } else {
        "attention"
    }
}

/// 一张表的列名（`PRAGMA table_info`；表不存在时返回空）。
fn columns_of(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote(table)))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(code: &'static str) -> ModuleIssue {
        ModuleIssue::new(code, "x")
    }

    #[test]
    fn verdict_keeps_three_levels() {
        assert_eq!(verdict_of(&[]), "ok");
        assert_eq!(verdict_of(&[issue("not_checked")]), "attention");
        assert_eq!(verdict_of(&[issue("degraded"), issue("table_limit")]), "attention");
        for code in ["corrupt", "kv_shape", "lost_tables", "junk_table"] {
            assert_eq!(verdict_of(&[issue(code)]), "damaged", "{code}");
        }
        // 只要有一条硬伤，别的都只是附注
        assert_eq!(verdict_of(&[issue("not_checked"), issue("kv_shape")]), "damaged");
    }
}
