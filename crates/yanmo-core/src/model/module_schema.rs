//! 模块数据结构升级：**核心只执行这几种结构动作，模块给不了 SQL 字符串**。
//!
//! # 为什么不是"模块交一段 SQL，核心跑"
//!
//! 那等于把模块库的写权限整段交出去：`DROP TABLE` / 跨命名空间写 / 触发器 / `ATTACH`
//! 都从这一格进来。这里改成**结构动作闭集**——建表 / 加列 / 建索引，表名与列名由核心
//! 拼上模块前缀并加引号，模块只给后缀。它表达力够模块用（地图的坐标点、人物关系、
//! 归纳结果都能建表），却拼不出第二条路。
//!
//! # 版本纪律
//!
//! 迁移必须**逐版**：当前是 v1，就只能交 v2；跳版、回退、重放都拒。所有步骤在
//! **一个事务**里跑，任何一步失败整体回滚，版本号不前进——"升到一半"比不升更坏。
//!
//! 模块表的命名空间规则见 [`crate::store::ModuleData`]：`mod_<模块id>_<表>`。

use serde::{Deserialize, Serialize};

use crate::error::{codes, Error, Result};

/// 表 / 索引 / 列名的最大长度（后缀部分；前缀由核心加）。
pub const MAX_SCHEMA_NAME_CHARS: usize = 48;
/// 一张表最多几列。
pub const MAX_SCHEMA_COLUMNS: usize = 32;
/// 一个索引最多几列。
pub const MAX_INDEX_COLUMNS: usize = 8;
/// 一次迁移最多几步（防止拿一个巨型清单拖死启动）。
pub const MAX_SCHEMA_STEPS: usize = 32;

/// 列类型闭集（SQLite 的五种存储类里取四种；`NULL` 不是列类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleColumnType {
    Integer,
    Text,
    Real,
    Blob,
}

impl ModuleColumnType {
    pub const fn as_sql(self) -> &'static str {
        match self {
            ModuleColumnType::Integer => "INTEGER",
            ModuleColumnType::Text => "TEXT",
            ModuleColumnType::Real => "REAL",
            ModuleColumnType::Blob => "BLOB",
        }
    }
}

/// 一列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleColumn {
    /// 列名（小写字母开头，只含小写字母 / 数字 / `_`）
    pub name: String,
    pub kind: ModuleColumnType,
    #[serde(default)]
    pub not_null: bool,
    #[serde(default)]
    pub primary_key: bool,
}

/// 一张表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleTable {
    /// 表名后缀（核心会拼成 `mod_<模块id>_<后缀>`）
    pub name: String,
    pub columns: Vec<ModuleColumn>,
}

/// 一步结构动作（闭集）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ModuleSchemaStep {
    /// 建表（表已存在会失败并整体回滚——版本号对不上时不该硬来）
    CreateTable { table: ModuleTable },
    /// 加一列（只能加可空列：SQLite 的 `ADD COLUMN` 不接受无默认值的 NOT NULL）
    AddColumn { table: String, column: ModuleColumn },
    /// 建索引（索引用同一个命名空间前缀）
    CreateIndex {
        name: String,
        table: String,
        columns: Vec<String>,
        #[serde(default)]
        unique: bool,
    },
}

/// 一次迁移：目标版本 + 步骤。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleMigration {
    /// 目标版本（必须正好是当前版本 + 1）
    pub version: i64,
    pub steps: Vec<ModuleSchemaStep>,
}

impl ModuleMigration {
    /// 只做**结构校验**（命名 / 数量 / 重复 / 主键）；表与列存不存在由执行层判。
    pub fn validate(&self) -> Result<()> {
        if self.version < 1 {
            return Err(step_invalid("version must be >= 1"));
        }
        if self.steps.is_empty() {
            return Err(step_invalid("a migration needs at least one step"));
        }
        if self.steps.len() > MAX_SCHEMA_STEPS {
            return Err(step_invalid(format!(
                "a migration may have at most {MAX_SCHEMA_STEPS} steps"
            )));
        }
        for step in &self.steps {
            match step {
                ModuleSchemaStep::CreateTable { table } => validate_table(table)?,
                ModuleSchemaStep::AddColumn { table, column } => {
                    validate_name(table, "table")?;
                    validate_column(column)?;
                    if column.primary_key {
                        return Err(step_invalid("an added column cannot be a primary key"));
                    }
                    if column.not_null {
                        return Err(step_invalid(
                            "an added column must be nullable (SQLite cannot add a NOT NULL column without a default)",
                        ));
                    }
                }
                ModuleSchemaStep::CreateIndex { name, table, columns, .. } => {
                    validate_name(name, "index")?;
                    validate_name(table, "table")?;
                    if columns.is_empty() || columns.len() > MAX_INDEX_COLUMNS {
                        return Err(step_invalid(format!(
                            "an index needs 1..={MAX_INDEX_COLUMNS} columns"
                        )));
                    }
                    let mut seen = std::collections::BTreeSet::new();
                    for column in columns {
                        validate_name(column, "index column")?;
                        if !seen.insert(column.as_str()) {
                            return Err(step_invalid(format!("duplicate index column: {column}")));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_table(table: &ModuleTable) -> Result<()> {
    validate_name(&table.name, "table")?;
    if table.columns.is_empty() || table.columns.len() > MAX_SCHEMA_COLUMNS {
        return Err(step_invalid(format!(
            "a table needs 1..={MAX_SCHEMA_COLUMNS} columns"
        )));
    }
    let mut names = std::collections::BTreeSet::new();
    let mut primary_keys = 0;
    for column in &table.columns {
        validate_column(column)?;
        if !names.insert(column.name.as_str()) {
            return Err(step_invalid(format!("duplicate column: {}", column.name)));
        }
        if column.primary_key {
            primary_keys += 1;
        }
    }
    if primary_keys > 1 {
        return Err(step_invalid("a table may have at most one primary key column"));
    }
    Ok(())
}

fn validate_column(column: &ModuleColumn) -> Result<()> {
    validate_name(&column.name, "column")
}

/// 名字合规：**小写字母开头，只含小写字母 / 数字 / `_`，≤48 字符**。
///
/// 这条比"SQL 标识符"窄得多是有意的：它同时挡住大小写撞车、连字符、引号、分号与
/// 路径分隔符——模块给的名字永远只是"一段后缀"，不是一个能拼进 SQL 的片段。
pub fn validate_name(name: &str, what: &'static str) -> Result<()> {
    let valid = (1..=MAX_SCHEMA_NAME_CHARS).contains(&name.chars().count())
        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(step_invalid(format!(
            "invalid {what} name: {name:?} (lowercase letter first, then a-z 0-9 _, max {MAX_SCHEMA_NAME_CHARS})"
        )))
    }
}

fn step_invalid(detail: impl Into<String>) -> Error {
    Error::invalid_with(codes::MODULE_SCHEMA_STEP_INVALID, [("detail", detail.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str) -> ModuleTable {
        ModuleTable {
            name: name.to_string(),
            columns: vec![
                ModuleColumn {
                    name: "id".to_string(),
                    kind: ModuleColumnType::Integer,
                    not_null: true,
                    primary_key: true,
                },
                ModuleColumn {
                    name: "label".to_string(),
                    kind: ModuleColumnType::Text,
                    not_null: false,
                    primary_key: false,
                },
            ],
        }
    }

    #[test]
    fn names_are_a_narrow_suffix_not_a_sql_fragment() {
        for good in ["places", "p2", "a_b"] {
            assert!(validate_name(good, "table").is_ok(), "应接受：{good}");
        }
        for bad in ["", "Places", "2places", "_places", "a-b", "a b", "a\"; DROP", &"x".repeat(49)] {
            assert!(validate_name(bad, "table").is_err(), "应拒绝：{bad:?}");
        }
    }

    #[test]
    fn migrations_are_structural_and_bounded() {
        let migration = ModuleMigration {
            version: 1,
            steps: vec![ModuleSchemaStep::CreateTable { table: table("places") }],
        };
        assert!(migration.validate().is_ok());

        let mut duplicate = migration.clone();
        if let ModuleSchemaStep::CreateTable { table } = &mut duplicate.steps[0] {
            table.columns.push(table.columns[1].clone());
        }
        assert!(duplicate.validate().is_err());

        let mut two_keys = migration.clone();
        if let ModuleSchemaStep::CreateTable { table } = &mut two_keys.steps[0] {
            table.columns[1].primary_key = true;
        }
        assert!(two_keys.validate().is_err());

        let mut not_null_add = migration.clone();
        not_null_add.version = 2;
        not_null_add.steps = vec![ModuleSchemaStep::AddColumn {
            table: "places".to_string(),
            column: ModuleColumn {
                name: "extra".to_string(),
                kind: ModuleColumnType::Text,
                not_null: true,
                primary_key: false,
            },
        }];
        assert!(not_null_add.validate().is_err());
    }

    #[test]
    fn a_migration_needs_a_version_and_steps() {
        let empty = ModuleMigration { version: 1, steps: Vec::new() };
        assert!(empty.validate().is_err());
        let zero = ModuleMigration {
            version: 0,
            steps: vec![ModuleSchemaStep::CreateTable { table: table("places") }],
        };
        assert!(zero.validate().is_err());
    }
}
