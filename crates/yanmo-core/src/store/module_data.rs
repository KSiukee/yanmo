//! 模块数据区（`modules.db`）：**核心治理、模块自用**的第二个库文件。
//!
//! # 为什么单独一个库
//!
//! 模块的数据与作者的稿子不是一回事：稿子的备份 / 校验 / 搬库必须永远带着它，
//! 模块数据可以单独勾选、单独清、单独导出。分成两个文件之后：
//! - 核心库坏了不连带模块数据，模块数据坏了也不挡核心打开；
//! - 备份是两次 `VACUUM INTO`，模块那份可以按用户勾选带不带；
//! - 模块命名空间（`mod_<module_id>_*`）与核心表天然分开，不会误伤。
//!
//! # 核心管什么、不管什么
//!
//! 管：文件位置、命名空间前缀、每个模块的 schema 版本账、表数量统计。
//! 不管：模块表的列与语义（那是模块自己的迁移），也**不在这里执行模块给的 SQL**——
//! 模块迁移必须由宿主在受控接口里跑（见数据写入护栏那一族）。
//!
//! # 命名空间
//!
//! 表名统一 `mod_<module_id>_<表>`。模块 id 本身可能带 `.` / `-`（反向域名式），
//! 它们在 SQLite 里是合法标识符（用时加引号）；后缀只允许小写字母、数字与 `_`，
//! 免得大小写与连字符把"这张表是谁的"搅浑。

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 模块数据区的句柄（**独立于核心库**）。
pub struct ModuleData {
    conn: Connection,
}

impl ModuleData {
    /// 与核心库并排的默认文件名（壳按数据目录拼出来）。
    pub fn path_beside(core_db: &Path) -> PathBuf {
        core_db.with_file_name("modules.db")
    }

    /// 打开（或创建）模块数据区，并确保记账表存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = crate::db::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS module_schema (
                module_id      TEXT PRIMARY KEY,
                schema_version INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    /// 模块表的统一前缀（`mod_<id>_`）。
    pub fn table_prefix(module_id: &str) -> String {
        format!("mod_{module_id}_")
    }

    /// 这张表是不是这个模块命名空间里的表。
    ///
    /// 后缀只认小写字母、数字与 `_`（见模块文档）；空后缀不算。
    pub fn is_module_table(module_id: &str, table: &str) -> bool {
        let prefix = Self::table_prefix(module_id);
        table.starts_with(&prefix)
            && table.len() > prefix.len()
            && table[prefix.len()..]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// 记下（或更新）一个模块的 schema 版本。
    ///
    /// 这是**模块自己的迁移账**：模块从 v1 升到 v2 时写 2；核心不解释这个数字，
    /// 只负责在卸载 / 备份 / 恢复时能说清"它的数据是哪一版"。
    pub fn record_schema(&mut self, module_id: &str, version: i64) -> Result<()> {
        ensure_id(module_id)?;
        if version < 0 {
            return Err(Error::invalid_with(
                codes::MODULE_SCHEMA_INVALID,
                [("version", version.to_string())],
            ));
        }
        self.conn.execute(
            "INSERT INTO module_schema(module_id, schema_version, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(module_id) DO UPDATE SET
                 schema_version = excluded.schema_version,
                 updated_at = excluded.updated_at",
            params![module_id, version, now_millis()],
        )?;
        Ok(())
    }

    /// 读一个模块的 schema 版本（没记过 = `None`）。
    pub fn schema_version(&self, module_id: &str) -> Result<Option<i64>> {
        ensure_id(module_id)?;
        let mut stmt = self
            .conn
            .prepare("SELECT schema_version FROM module_schema WHERE module_id = ?1")?;
        let mut rows = stmt.query([module_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// 这个模块命名空间里现有的表（按名字排序）。
    ///
    /// 用 Rust 侧过滤而不是 SQL `LIKE`：模块 id 里有 `.` / `-`，而 `_` 在 `LIKE`
    /// 里是通配符，拼模式迟早会误伤别的模块。
    pub fn tables_of(&self, module_id: &str) -> Result<Vec<String>> {
        ensure_id(module_id)?;
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(names
            .into_iter()
            .filter(|table| Self::is_module_table(module_id, table))
            .collect())
    }

    /// 这个模块命名空间里现有几张表。
    pub fn table_count(&self, module_id: &str) -> Result<usize> {
        Ok(self.tables_of(module_id)?.len())
    }
}

fn ensure_id(module_id: &str) -> Result<()> {
    if yanmo_proto::is_valid_module_id(module_id) {
        Ok(())
    } else {
        Err(Error::invalid_with(
            codes::MODULE_ID_INVALID,
            [("module_id", module_id.to_string())],
        ))
    }
}
