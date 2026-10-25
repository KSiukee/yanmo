//! 模块数据区的写护栏：**硬容量、表/行/单次写入/频率**。
//!
//! 这一层先给数据层和将来宿主 ABI 共用；所有限制都在**核心侧**执行，
//! 模块拿不到任何可以绕过的写入句柄。数据库硬上限用 SQLite 自己的
//! `max_page_count` 兜底——写爆时数据库直接报 `SQLITE_FULL`，不会把磁盘吃光。

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;

use super::module_data::{ensure_id, quote, table_ident, ModuleData};
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

const MAX_KEY_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDataLimits {
    /// 整个 `modules.db` 的硬上限（字节）
    pub total_bytes: u64,
    /// 单模块 KV 数据总字节上限
    pub max_module_bytes: u64,
    /// 单个模块最多允许的表数
    pub max_tables: usize,
    /// 单表最多行数
    pub max_rows_per_table: u64,
    /// 单次写入的 value 上限（字节）
    pub max_write_bytes: usize,
    /// 每分钟最多写入次数
    pub max_writes_per_minute: usize,
    /// SQLite busy timeout（毫秒）
    pub busy_timeout_ms: u64,
}

impl Default for ModuleDataLimits {
    fn default() -> Self {
        Self {
            total_bytes: 256 * 1024 * 1024,
            max_module_bytes: 8 * 1024 * 1024,
            max_tables: 32,
            max_rows_per_table: 100_000,
            max_write_bytes: 1024 * 1024,
            max_writes_per_minute: 120,
            busy_timeout_ms: 2_000,
        }
    }
}

impl ModuleDataLimits {
    pub fn validate(&self) -> Result<()> {
        if self.total_bytes == 0
            || self.max_module_bytes == 0
            || self.max_tables == 0
            || self.max_rows_per_table == 0
            || self.max_write_bytes == 0
            || self.max_writes_per_minute == 0
        {
            return Err(Error::invalid(codes::MODULE_DATA_WRITE_LIMIT));
        }
        Ok(())
    }
}

pub(super) struct WriteGuard {
    writes: VecDeque<Instant>,
}

impl WriteGuard {
    pub(super) fn new() -> Self {
        Self { writes: VecDeque::new() }
    }

    fn allowed(&mut self, per_minute: usize, now: Instant) -> Result<()> {
        while let Some(first) = self.writes.front() {
            if now.duration_since(*first) >= Duration::from_secs(60) {
                self.writes.pop_front();
            } else {
                break;
            }
        }
        if self.writes.len() >= per_minute {
            return Err(Error::invalid(codes::MODULE_DATA_RATE_LIMIT));
        }
        Ok(())
    }

    fn record(&mut self, now: Instant) {
        self.writes.push_back(now);
    }
}

impl ModuleData {
    /// 打开并覆盖默认限制。调用方必须在任何写入之前调用。
    pub fn open_with_limits(path: impl AsRef<Path>, limits: ModuleDataLimits) -> Result<Self> {
        limits.validate()?;
        let mut data = Self::open(path)?;
        data.limits = limits;
        data.apply_limits()?;
        Ok(data)
    }

    pub(super) fn apply_limits(&self) -> Result<()> {
        self.conn.busy_timeout(Duration::from_millis(self.limits.busy_timeout_ms))?;
        let page_size: u64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap_or(4096);
        let max_pages = (self.limits.total_bytes / page_size.max(1)).max(1);
        self.conn
            .pragma_update(None, "max_page_count", max_pages as i64)?;
        Ok(())
    }

    /// 只给宿主 ABI 用：写 key/value（key 是 UTF-8 文本，value 是字节串）。
    pub fn kv_put(&mut self, module_id: &str, key: &str, value: &[u8]) -> Result<()> {
        ensure_id(module_id)?;
        self.ensure_not_isolated(module_id)?;
        if key.is_empty() || key.len() > MAX_KEY_BYTES || value.len() > self.limits.max_write_bytes {
            return Err(Error::invalid(codes::MODULE_DATA_WRITE_LIMIT));
        }
        let table = table_ident(module_id, "kv");
        self.ensure_kv_table(module_id, &table)?;
        let rows: u64 = self
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {}", quote(&table)), [], |row| row.get::<_, i64>(0))
            .map(|value| value.max(0) as u64)
            .unwrap_or(0);
        let exists: Option<i64> = self
            .conn
            .query_row(
                &format!("SELECT 1 FROM {} WHERE key = ?1", quote(&table)),
                [key],
                |row| row.get(0),
            )
            .optional()?;
        let old_size: u64 = if exists.is_some() {
            self.conn
                .query_row(
                    &format!("SELECT length(value) FROM {} WHERE key = ?1", quote(&table)),
                    [key],
                    |row| row.get::<_, i64>(0),
                )
                .map(|value| value.max(0) as u64)
                .unwrap_or(0)
        } else {
            0
        };
        let total: u64 = self
            .conn
            .query_row(
                &format!("SELECT COALESCE(SUM(length(value)), 0) FROM {}", quote(&table)),
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value.max(0) as u64)
            .unwrap_or(0);
        if total.saturating_sub(old_size).saturating_add(value.len() as u64)
            > self.limits.max_module_bytes
        {
            return Err(Error::invalid_with(
                codes::MODULE_DATA_MODULE_QUOTA,
                [("module_id", module_id.to_string())],
            ));
        }
        let rows_after = rows + u64::from(exists.is_none());
        self.check_write(module_id, &table, rows_after, value.len())?;
        self.conn.execute(
            &format!(
                "INSERT INTO {} (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                quote(&table)
            ),
            rusqlite::params![key, value, now_millis()],
        )?;
        self.guard.record(Instant::now());
        Ok(())
    }

    pub fn kv_get(&self, module_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        ensure_id(module_id)?;
        self.ensure_not_isolated(module_id)?;
        if key.is_empty() || key.len() > MAX_KEY_BYTES {
            return Err(Error::invalid(codes::MODULE_DATA_WRITE_LIMIT));
        }
        let table = table_ident(module_id, "kv");
        if !self.table_exists(&table)? {
            return Ok(None);
        }
        self.conn
            .query_row(
                &format!("SELECT value FROM {} WHERE key = ?1", quote(&table)),
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// 列出模块自己 KV 里的键（排序稳定；还没写过就是空）。
    ///
    /// 只读：给"模块界面看自己的数据"用。隔离中的模块**读不了**（与 [`Self::kv_get`] 同口径）。
    pub fn kv_keys(&self, module_id: &str) -> Result<Vec<String>> {
        ensure_id(module_id)?;
        self.ensure_not_isolated(module_id)?;
        let table = table_ident(module_id, "kv");
        if !self.table_exists(&table)? {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT key FROM {} ORDER BY key", quote(&table)))?;
        let keys = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(keys)
    }

    pub fn kv_delete(&mut self, module_id: &str, key: &str) -> Result<bool> {
        ensure_id(module_id)?;
        self.ensure_not_isolated(module_id)?;
        if key.is_empty() || key.len() > MAX_KEY_BYTES {
            return Err(Error::invalid(codes::MODULE_DATA_WRITE_LIMIT));
        }
        let table = table_ident(module_id, "kv");
        if !self.table_exists(&table)? {
            return Ok(false);
        }
        let changed = self.conn.execute(
            &format!("DELETE FROM {} WHERE key = ?1", quote(&table)),
            [key],
        )?;
        Ok(changed > 0)
    }

    fn ensure_kv_table(&mut self, module_id: &str, table: &str) -> Result<()> {
        if self.table_exists(table)? {
            return Ok(());
        }
        if self.tables_of(module_id)?.len() >= self.limits.max_tables {
            return Err(Error::invalid_with(
                codes::MODULE_DATA_TABLE_LIMIT,
                [("module_id", module_id.to_string())],
            ));
        }
        self.conn.execute(
            &format!(
                "CREATE TABLE {} (
                    key        TEXT PRIMARY KEY,
                    value      BLOB NOT NULL,
                    updated_at INTEGER NOT NULL
                )",
                quote(table)
            ),
            [],
        )?;
        Ok(())
    }

    pub(super) fn table_exists(&self, table: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    fn check_write(&mut self, module_id: &str, table: &str, rows_after: u64, bytes: usize) -> Result<()> {
        if !Self::is_module_table(module_id, table) {
            return Err(Error::invalid(codes::MODULE_DATA_WRITE_LIMIT));
        }
        if rows_after > self.limits.max_rows_per_table {
            return Err(Error::invalid_with(
                codes::MODULE_DATA_ROW_LIMIT,
                [("module_id", module_id.to_string())],
            ));
        }
        if bytes > self.limits.max_write_bytes {
            return Err(Error::invalid(codes::MODULE_DATA_WRITE_LIMIT));
        }
        self.guard
            .allowed(self.limits.max_writes_per_minute, Instant::now())
    }
}
