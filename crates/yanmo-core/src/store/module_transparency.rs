//! 模块透明度的事实面：**空间 / 行为 / 状态，全部是核心自己观测到的东西**。
//!
//! 这一层只回答「这一格现在是什么样」，不评分、不判断安不安全——判断留给作者
//! （透明度面板的三条呈现纪律：一句话总结优先 / 只给事实不做评分 / 标明谁说的）。
//!
//! 口径写清楚，免得界面拿错数：
//! - `data_bytes` 说的是**值里存了多少字节**（KV 口径：`length(value) + length(key)`）。
//!   模块要是有不带 `value` 列的自定义表，这一项就只算了 KV——那时 `data_bytes_partial`
//!   为真，界面必须说明「只算了 KV」，不许拿它当"这个模块占了多少"。
//! - `db_bytes` 是整个 `modules.db`（含 WAL），**不是单个模块的**，别混用。
//! - `mirror_written_at` 取镜像文件的修改时刻（没写过 = 0），不读文件内容。

use serde::Serialize;

use super::module_data::{ensure_id, quote, ModuleData};
use crate::error::Result;

/// 一个模块的事实快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleFacts {
    pub module_id: String,
    /// 命名空间里的表数。
    pub tables: usize,
    /// 这些表的行数合计。
    pub rows: i64,
    /// 值里存的字节数（见模块文档的口径）。
    pub data_bytes: u64,
    /// 有表没有 `value` 列 → 这个字节数只算了 KV。
    pub data_bytes_partial: bool,
    /// 整个 `modules.db` 的字节数（含 WAL；**不是单个模块的**）。
    pub db_bytes: u64,
    /// 结构版本（没记过 = `None`）。
    pub schema_version: Option<i64>,
    /// 写守卫降级过没有。
    pub degraded: bool,
    pub rejected: u64,
    /// 数据是否被隔离。
    pub isolated: bool,
    pub isolation_reason: String,
    /// 镜像文件的修改时刻（毫秒；0 = 还没有镜像）。
    pub mirror_written_at: i64,
    /// 最后一次启动的时刻（毫秒；0 = 从没跑过）。
    pub last_run_at: i64,
    pub runs: u64,
}

impl ModuleData {
    /// 读这一格的事实（只读，一个字节都不改）。
    pub fn facts(&self, module_id: &str) -> Result<ModuleFacts> {
        ensure_id(module_id)?;
        let stats = self.stats(module_id)?;
        let health = self.health(module_id)?;
        let isolation = self.isolation(module_id)?;
        let usage = self.usage(module_id)?;
        let (data_bytes, data_bytes_partial) = self.namespace_bytes(module_id)?;
        Ok(ModuleFacts {
            module_id: module_id.to_string(),
            tables: stats.tables,
            rows: stats.rows,
            data_bytes,
            data_bytes_partial,
            db_bytes: stats.db_bytes,
            schema_version: self.schema_version(module_id)?,
            degraded: health.degraded,
            rejected: health.rejected,
            isolated: isolation.is_some(),
            isolation_reason: isolation.map(|state| state.reason).unwrap_or_default(),
            mirror_written_at: mirror_mtime_ms(&self.mirror_path(module_id)),
            last_run_at: usage.last_run_at,
            runs: usage.runs,
        })
    }

    /// 命名空间里「值里存了多少字节」（以及是否只算了 KV）。
    fn namespace_bytes(&self, module_id: &str) -> Result<(u64, bool)> {
        let mut total: u64 = 0;
        let mut partial = false;
        for table in self.tables_of(module_id)? {
            let columns = column_names(&self.conn, &table)?;
            if !columns.iter().any(|column| column == "value") {
                partial = true;
                continue;
            }
            total = total.saturating_add(sum_length(&self.conn, &table, "value")?);
            if columns.iter().any(|column| column == "key") {
                total = total.saturating_add(sum_length(&self.conn, &table, "key")?);
            }
        }
        Ok((total, partial))
    }
}

/// 一张表的列名（`PRAGMA table_info`）。
fn column_names(conn: &rusqlite::Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote(table)))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// 某一列的长度合计（行里有 NULL 时 `LENGTH` 是 NULL，`COALESCE` 兜住）。
fn sum_length(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<u64> {
    let total: i64 = conn.query_row(
        &format!(
            "SELECT COALESCE(SUM(LENGTH({})), 0) FROM {}",
            quote(column),
            quote(table)
        ),
        [],
        |row| row.get(0),
    )?;
    Ok(total.max(0) as u64)
}

/// 文件修改时刻（毫秒；读不到就是 0）。
fn mirror_mtime_ms(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
