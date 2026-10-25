//! 宿主侧的模块数据 KV（`modules.db`）：与核心侧 `ModuleData` 同口径的最小子集。
//!
//! 宿主 crate 不链接 `yanmo-core`（结构约束），所以这里只实现 `env.data_*` 需要的
//! KV 写面：整库硬上限、单模块字节、单次 value、每分钟写入。表名仍由宿主拼，
//! guest 只能给 key/value。

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{HostError, HostResult};

const TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MODULE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ROWS: u64 = 100_000;
const MAX_WRITE_BYTES: usize = 1024 * 1024;
const MAX_WRITES_PER_MINUTE: usize = 120;
const MAX_KEY_BYTES: usize = 128;

pub struct ModuleData {
    conn: Connection,
    writes: VecDeque<Instant>,
    rejected: usize,
    degraded: bool,
}

impl ModuleData {
    pub fn open(path: impl AsRef<Path>) -> HostResult<Self> {
        let conn = Connection::open(path).map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        conn.busy_timeout(Duration::from_secs(2))
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        let page_size: u64 = conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap_or(4096);
        let max_pages = (TOTAL_BYTES / page_size.max(1)).max(1);
        conn.pragma_update(None, "max_page_count", max_pages as i64)
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS module_data_health (
                module_id TEXT PRIMARY KEY,
                degraded  INTEGER NOT NULL,
                rejected  INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        Ok(Self { conn, writes: VecDeque::new(), rejected: 0, degraded: false })
    }

    /// 连续 5 次写被拒就降级：只读，后续 put/delete 直接 -1（用户仍可导出/清除）。
    pub fn kv_put(&mut self, module_id: &str, key: &str, value: &[u8]) -> HostResult<()> {
        self.load_health(module_id)?;
        if self.degraded {
            self.rejected += 1;
            return Err(HostError::new(HostError::DATA_LIMIT, "module data degraded"));
        }
        let result = self.kv_put_inner(module_id, key, value);
        if result.is_err() {
            self.note_failure(module_id);
        }
        result
    }

    fn kv_put_inner(&mut self, module_id: &str, key: &str, value: &[u8]) -> HostResult<()> {
        if key.is_empty() || key.len() > MAX_KEY_BYTES || value.len() > MAX_WRITE_BYTES {
            return Err(HostError::new(HostError::DATA_LIMIT, "key/value too large"));
        }
        let table = table(module_id);
        self.ensure_table(&table)?;
        let rows: u64 = self
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {}", quote(&table)), [], |row| row.get::<_, i64>(0))
            .map(|value| value.max(0) as u64)
            .unwrap_or(0);
        let old_size: u64 = self
            .conn
            .query_row(
                &format!("SELECT length(value) FROM {} WHERE key = ?1", quote(&table)),
                [key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?
            .map(|value| value.max(0) as u64)
            .unwrap_or(0);
        let total: u64 = self
            .conn
            .query_row(
                &format!("SELECT COALESCE(SUM(length(value)), 0) FROM {}", quote(&table)),
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value.max(0) as u64)
            .unwrap_or(0);
        let rows_after = rows + u64::from(old_size == 0);
        if rows_after > MAX_ROWS {
            return Err(HostError::new(HostError::DATA_LIMIT, "row limit"));
        }
        if total.saturating_sub(old_size).saturating_add(value.len() as u64) > MODULE_BYTES {
            return Err(HostError::new(HostError::DATA_LIMIT, "module quota"));
        }
        self.rate()?;
        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    quote(&table)
                ),
                params![key, value, now_millis()],
            )
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        self.writes.push_back(Instant::now());
        Ok(())
    }

    pub fn kv_get(&self, module_id: &str, key: &str) -> HostResult<Option<Vec<u8>>> {
        if key.is_empty() || key.len() > MAX_KEY_BYTES {
            return Err(HostError::new(HostError::DATA_LIMIT, "key too large"));
        }
        let table = table(module_id);
        if !self.exists(&table)? {
            return Ok(None);
        }
        self.conn
            .query_row(
                &format!("SELECT value FROM {} WHERE key = ?1", quote(&table)),
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))
    }

    pub fn kv_delete(&mut self, module_id: &str, key: &str) -> HostResult<bool> {
        self.load_health(module_id)?;
        if self.degraded {
            self.rejected += 1;
            return Err(HostError::new(HostError::DATA_LIMIT, "module data degraded"));
        }
        let result = self.kv_delete_inner(module_id, key);
        if result.is_err() {
            self.note_failure(module_id);
        }
        result
    }

    fn kv_delete_inner(&mut self, module_id: &str, key: &str) -> HostResult<bool> {
        if key.is_empty() || key.len() > MAX_KEY_BYTES {
            return Err(HostError::new(HostError::DATA_LIMIT, "key too large"));
        }
        let table = table(module_id);
        if !self.exists(&table)? {
            return Ok(false);
        }
        let changed = self
            .conn
            .execute(&format!("DELETE FROM {} WHERE key = ?1", quote(&table)), [key])
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        Ok(changed > 0)
    }

    fn ensure_table(&mut self, table: &str) -> HostResult<()> {
        if self.exists(table)? {
            return Ok(());
        }
        self.conn
            .execute(
                &format!(
                    "CREATE TABLE {} (
                        key        TEXT PRIMARY KEY,
                        value      BLOB NOT NULL,
                        updated_at INTEGER NOT NULL
                    )",
                    quote(table)
                ),
                [],
            )
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        Ok(())
    }

    fn exists(&self, table: &str) -> HostResult<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value.is_some())
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))
    }

    pub fn rejected(&self) -> usize {
        self.rejected
    }

    pub fn degraded(&self) -> bool {
        self.degraded
    }

    /// 跨轮健康：宿主写入，核心/界面只读；清除数据时一起清。
    pub fn health(&self, module_id: &str) -> HostResult<(bool, u64)> {
        let found: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT degraded, rejected FROM module_data_health WHERE module_id = ?1",
                [module_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| HostError::new(HostError::DATA_FAILED, error.to_string()))?;
        Ok(found
            .map(|(degraded, rejected)| (degraded != 0, rejected.max(0) as u64))
            .unwrap_or((false, 0)))
    }

    fn load_health(&mut self, module_id: &str) -> HostResult<()> {
        let (degraded, rejected) = self.health(module_id)?;
        self.degraded |= degraded;
        self.rejected = self.rejected.max(rejected as usize);
        Ok(())
    }

    fn note_failure(&mut self, module_id: &str) {
        self.rejected += 1;
        if self.rejected >= 5 {
            self.degraded = true;
        }
        let _ = self.conn.execute(
            "INSERT INTO module_data_health(module_id, degraded, rejected, updated_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(module_id) DO UPDATE SET
               degraded = excluded.degraded,
               rejected = excluded.rejected,
               updated_at = excluded.updated_at",
            params![
                module_id,
                if self.degraded { 1 } else { 0 },
                self.rejected as i64,
                now_millis()
            ],
        );
    }

    fn rate(&mut self) -> HostResult<()> {
        let now = Instant::now();
        while let Some(first) = self.writes.front() {
            if now.duration_since(*first) >= Duration::from_secs(60) {
                self.writes.pop_front();
            } else {
                break;
            }
        }
        if self.writes.len() >= MAX_WRITES_PER_MINUTE {
            return Err(HostError::new(HostError::DATA_LIMIT, "write rate"));
        }
        Ok(())
    }
}

fn table(module_id: &str) -> String {
    format!("mod_{module_id}_kv")
}

fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
