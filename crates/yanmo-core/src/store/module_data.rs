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
//! 管：文件位置、命名空间前缀、每个模块的 schema 版本账、**受控的结构升级**
//! （建表 / 加列 / 建索引，见 [`ModuleMigration`]）、表数与行数统计。
//! 不管：模块表的语义；也**不执行模块给的 SQL 字符串**——升级只接受结构动作，
//! 表名 / 列名由核心拼前缀、加引号，模块给的名字永远只是一段后缀。
//!
//! # 命名空间
//!
//! 表名统一 `mod_<module_id>_<表>`。模块 id 本身可能带 `.` / `-`（反向域名式），
//! 它们在 SQLite 里是合法标识符（用时加引号）；后缀只允许小写字母、数字与 `_`，
//! 免得大小写与连字符把"这张表是谁的"搅浑。

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::{codes, Error, Result};
use crate::model::{ModuleColumn, ModuleMigration, ModuleSchemaStep, ModuleTable};
use crate::time::now_millis;

use super::module_data_limits::{ModuleDataLimits, WriteGuard};
use super::module_isolation::ISOLATION_TABLE_SQL;
use super::module_namespace::{classify_table, TableOwner};

/// 模块数据区的句柄（**独立于核心库**）。
pub struct ModuleData {
    pub(super) conn: Connection,
    pub(super) path: PathBuf,
    pub(super) limits: ModuleDataLimits,
    pub(super) guard: WriteGuard,
}

/// 一个模块的数据规模（**按需算，不落库**，避免计数器过期）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleStats {
    /// 这个模块命名空间里的表数
    pub tables: usize,
    /// 这些表里的行数合计（`COUNT(*)` 之和）
    pub rows: i64,
    /// 整个 `modules.db`（含 `-wal`）的字节数——**不是这个模块单独的**
    pub db_bytes: u64,
}

impl ModuleData {
    /// 与核心库并排的默认文件名（壳按数据目录拼出来）。
    pub fn path_beside(core_db: &Path) -> PathBuf {
        core_db.with_file_name("modules.db")
    }

    /// 打开（或创建）模块数据区，并确保记账表存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = crate::db::open(&path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS module_schema (
                module_id      TEXT PRIMARY KEY,
                schema_version INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS module_data_health (
                module_id TEXT PRIMARY KEY,
                degraded  INTEGER NOT NULL,
                rejected  INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        // 隔离账独立一张表（而不是给健康表加列）：老库打开时自动补上，不需要迁移。
        conn.execute(ISOLATION_TABLE_SQL, [])?;
        let limits = ModuleDataLimits::default();
        let data = Self {
            conn,
            path,
            limits,
            guard: WriteGuard::new(),
        };
        data.apply_limits()?;
        Ok(data)
    }

    /// 这个数据区的文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 模块表的统一前缀（`mod_<id>_`）。
    pub fn table_prefix(module_id: &str) -> String {
        format!("mod_{module_id}_")
    }

    /// 这张表是不是这个模块命名空间里的表。
    ///
    /// 判定只有一处（`module_namespace::classify_table`）：`mod_` 之后的第一个下划线
    /// 切出 id 与后缀，后缀只认小写字母、数字与 `_`，空后缀不算。**不另写一套前缀比较**——
    /// 两套口径迟早会分叉，那时"核心认不出、模块却以为写了"就会变成一类难查的故障。
    pub fn is_module_table(module_id: &str, table: &str) -> bool {
        matches!(
            classify_table(table),
            TableOwner::Module { module_id: found, .. } if found == module_id
        )
    }

    /// 记下（或更新）一个模块的 schema 版本（**只给测试 / 恢复工具用**）。
    ///
    /// 正常升级走 [`ModuleData::apply_migration`]：它把结构变更与版本号放在同一个事务里。
    /// 这里单开一条是给"从备份恢复 / 手工修账"留的口子，别拿它绕过版本纪律。
    pub fn record_schema(&mut self, module_id: &str, version: i64) -> Result<()> {
        ensure_id(module_id)?;
        Self::record_schema_in(&self.conn, module_id, version)
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

    /// 执行一次**受控**的模块结构升级。
    ///
    /// 三条纪律：
    /// 1. **逐版**：当前是 v1 就只能交 v2；跳版 / 回退 / 重放都拒；
    /// 2. **整体事务**：任何一步失败整体回滚，版本号不前进；
    /// 3. **没有任意 SQL**：只执行建表 / 加列 / 建索引，名字由核心校验并加前缀。
    pub fn apply_migration(
        &mut self,
        module_id: &str,
        migration: &ModuleMigration,
    ) -> Result<()> {
        ensure_id(module_id)?;
        self.ensure_not_isolated(module_id)?;
        migration.validate()?;

        let current = self.schema_version(module_id)?.unwrap_or(0);
        let expected = current + 1;
        if migration.version != expected {
            return Err(Error::invalid_with(
                codes::MODULE_SCHEMA_VERSION,
                [
                    ("current", current.to_string()),
                    ("found", migration.version.to_string()),
                ],
            ));
        }

        let created = migration
            .steps
            .iter()
            .filter(|step| matches!(step, ModuleSchemaStep::CreateTable { .. }))
            .count();
        if self.tables_of(module_id)?.len() + created > self.limits.max_tables {
            return Err(Error::invalid_with(
                codes::MODULE_DATA_TABLE_LIMIT,
                [("module_id", module_id.to_string())],
            ));
        }

        let mut statements = Vec::with_capacity(migration.steps.len());
        for step in &migration.steps {
            match step {
                ModuleSchemaStep::CreateTable { table } => {
                    statements.push(create_table_sql(module_id, table));
                }
                ModuleSchemaStep::AddColumn { table, column } => {
                    statements.push(format!(
                        "ALTER TABLE {} ADD COLUMN {} {}",
                        quote(&table_ident(module_id, table)),
                        quote(&column.name),
                        column.kind.as_sql()
                    ));
                }
                ModuleSchemaStep::CreateIndex { name, table, columns, unique } => {
                    let columns = columns
                        .iter()
                        .map(|column| quote(column))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!(
                        "CREATE {}INDEX {} ON {} ({columns})",
                        if *unique { "UNIQUE " } else { "" },
                        quote(&index_ident(module_id, name)),
                        quote(&table_ident(module_id, table))
                    ));
                }
            }
        }

        let tx = self.conn.transaction()?;
        for statement in &statements {
            tx.execute(statement, []).map_err(|error| {
                Error::invalid_with(codes::MODULE_SCHEMA_FAILED, [("detail", error.to_string())])
            })?;
        }
        Self::record_schema_in(&tx, module_id, migration.version)?;
        tx.commit()?;
        Ok(())
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

    /// 按需算这个模块的数据规模（表数 + 行数 + 整库体积）。
    pub fn stats(&self, module_id: &str) -> Result<ModuleStats> {
        let tables = self.tables_of(module_id)?;
        let mut rows: i64 = 0;
        for table in &tables {
            let count: i64 = self.conn.query_row(
                &format!("SELECT COUNT(*) FROM {}", quote(table)),
                [],
                |row| row.get(0),
            )?;
            rows = rows.saturating_add(count);
        }
        Ok(ModuleStats { tables: tables.len(), rows, db_bytes: self.file_bytes() })
    }

    /// 整个数据区的字节数（主文件 + `-wal`；WAL 没 checkpoint 时不能只看主文件）。
    pub fn file_bytes(&self) -> u64 {
        let main = std::fs::metadata(&self.path).map(|meta| meta.len()).unwrap_or(0);
        let mut wal = self.path.clone().into_os_string();
        wal.push("-wal");
        let wal = std::fs::metadata(PathBuf::from(wal)).map(|meta| meta.len()).unwrap_or(0);
        main.saturating_add(wal)
    }

    /// 把模块数据区做一份**一致性快照**（`VACUUM INTO`；目标文件必须不存在）。
    ///
    /// 备份链与"导出模块数据"共用这一条：它写出来的是自洽的单文件库，
    /// **不是**把正在写的 `modules.db`（连同 `-wal`）直接复制一份。
    pub fn snapshot_into(&self, target: impl AsRef<Path>) -> Result<()> {
        let target = target.as_ref();
        if target.exists() {
            return Err(Error::invalid_with(
                codes::MODULE_SNAPSHOT_FAILED,
                [("detail", format!("target exists: {}", target.display()))],
            ));
        }
        self.conn
            .execute("VACUUM INTO ?1", params![target.to_string_lossy().to_string()])
            .map_err(|error| {
                Error::invalid_with(codes::MODULE_SNAPSHOT_FAILED, [("detail", error.to_string())])
            })?;
        Ok(())
    }

    /// 删掉一个模块的**全部数据**（显式动作；卸载默认不调它）。
    ///
    /// 这是"连数据一起删"的唯一入口：先由界面二次确认，再调它。整个删除在一个事务里，
    /// 任何一张表删不掉（比如被别的表引用）就整体回滚，不留半套。
    /// 返回删掉了几张表；`module_schema` 里那一行、健康账与**隔离状态**也一起清掉
    /// （数据都没了，隔离自然失效；重装后是干净的一格）。
    pub fn drop_namespace(&mut self, module_id: &str) -> Result<usize> {
        ensure_id(module_id)?;
        let tables = self.tables_of(module_id)?;
        let tx = self.conn.transaction()?;
        for table in &tables {
            tx.execute(&format!("DROP TABLE {}", quote(table)), []).map_err(|error| {
                Error::invalid_with(
                    codes::MODULE_DROP_DATA_FAILED,
                    [("detail", error.to_string())],
                )
            })?;
        }
        tx.execute("DELETE FROM module_data_health WHERE module_id = ?1", [module_id])
            .map_err(|error| {
                Error::invalid_with(
                    codes::MODULE_DROP_DATA_FAILED,
                    [("detail", error.to_string())],
                )
            })?;
        tx.execute("DELETE FROM module_data_isolation WHERE module_id = ?1", [module_id])
            .map_err(|error| {
                Error::invalid_with(
                    codes::MODULE_DROP_DATA_FAILED,
                    [("detail", error.to_string())],
                )
            })?;
        tx.execute("DELETE FROM module_schema WHERE module_id = ?1", [module_id])
            .map_err(|error| {
                Error::invalid_with(
                    codes::MODULE_DROP_DATA_FAILED,
                    [("detail", error.to_string())],
                )
            })?;
        tx.commit()?;
        Ok(tables.len())
    }

    fn record_schema_in(conn: &Connection, module_id: &str, version: i64) -> Result<()> {
        if version < 0 {
            return Err(Error::invalid_with(
                codes::MODULE_SCHEMA_INVALID,
                [("version", version.to_string())],
            ));
        }
        conn.execute(
            "INSERT INTO module_schema(module_id, schema_version, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(module_id) DO UPDATE SET
                 schema_version = excluded.schema_version,
                 updated_at = excluded.updated_at",
            params![module_id, version, now_millis()],
        )?;
        Ok(())
    }
}

pub(super) fn ensure_id(module_id: &str) -> Result<()> {
    if yanmo_proto::is_valid_module_id(module_id) {
        Ok(())
    } else {
        Err(Error::invalid_with(
            codes::MODULE_ID_INVALID,
            [("module_id", module_id.to_string())],
        ))
    }
}

/// 表名：`mod_<模块id>_<后缀>`。
pub(super) fn table_ident(module_id: &str, suffix: &str) -> String {
    format!("mod_{module_id}_{suffix}")
}

/// 索引名：与表同一个命名空间前缀。
fn index_ident(module_id: &str, suffix: &str) -> String {
    format!("mod_{module_id}_{suffix}")
}

/// SQL 标识符加引号（名字已被校验成窄字符集；这里仍转义，防的是将来放宽校验）。
pub(super) fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn create_table_sql(module_id: &str, table: &ModuleTable) -> String {
    let columns = table
        .columns
        .iter()
        .map(|column| column_sql(column))
        .collect::<Vec<_>>()
        .join(", ");
    format!("CREATE TABLE {} ({columns})", quote(&table_ident(module_id, &table.name)))
}

fn column_sql(column: &ModuleColumn) -> String {
    let mut sql = format!("{} {}", quote(&column.name), column.kind.as_sql());
    if column.not_null || column.primary_key {
        sql.push_str(" NOT NULL");
    }
    if column.primary_key {
        sql.push_str(" PRIMARY KEY");
    }
    sql
}
