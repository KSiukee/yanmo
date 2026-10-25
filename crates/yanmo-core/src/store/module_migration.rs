//! 模块结构升级：**受控**的建表 / 加列 / 建索引。
//!
//! 三条纪律：**逐版**（当前 v1 就只能交 v2）/ **整体事务**（任何一步失败整体回滚，
//! 版本号不前进）/ **没有任意 SQL**（只执行建表、加列、建索引三种动作，名字由核心
//! 加前缀并转义，模块给的名字永远只是一段后缀）。
//!
//! 从 [`super::module_data`] 里分出来，只因为它单独就是一件事（结构升级）；
//! 那边已经装着打开、账本、规模、快照与删除五件事。

use super::module_data::{ensure_id, quote, table_ident, ModuleData};
use crate::error::{codes, Error, Result};
use crate::model::{ModuleColumn, ModuleMigration, ModuleSchemaStep, ModuleTable};

impl ModuleData {
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
}

/// 索引名：与表同一个命名空间前缀。
fn index_ident(module_id: &str, suffix: &str) -> String {
    format!("mod_{module_id}_{suffix}")
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
