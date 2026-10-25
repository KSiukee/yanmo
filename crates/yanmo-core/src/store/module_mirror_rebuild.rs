//! 从镜像重建一个模块的命名空间：**照着 JSON 把那一格重新搭起来**。
//!
//! # 两道门
//!
//! 1. **只重建自己那一格**：每条表名都必须在模块命名空间里（镜像读取时已校验）；
//! 2. **不许盖活数据**：命名空间里还有表、而这个模块**没被隔离**时直接拒——
//!    重建会先删后建，对着正在用的数据做这件事等于把「恢复」变成「破坏」。
//!    命名空间已经空了（比如刚清过、或从别的机器搬回来）则放行。
//!
//! # 不恢复什么
//!
//! 索引 / 约束 / 触发器 / 自增序列：镜像记的是**列与行**。这与「按模块导出」同一条边界；
//! 需要逐字结构时用整库快照。
//!
//! # 一步到位
//!
//! 删表、建表、灌行、写回 schema 版本全在**同一个事务**里：中途任何一步失败整份回滚，
//! 不会留下「删了旧的、新的没建成」的半套。

use serde::Serialize;

use super::module_data::{ensure_id, quote, ModuleData};
use super::module_mirror::{MirrorColumn, MirrorTable};
use super::module_mirror_cells::decode_cell;
use crate::error::{codes, Error, Result};

/// 一次重建的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleRebuild {
    pub module_id: String,
    pub tables: usize,
    pub rows: i64,
    /// 从镜像恢复的 schema 版本（镜像里没记就是 `None`）。
    pub schema_version: Option<i64>,
}

impl ModuleData {
    /// 用镜像重建这个模块的全部数据。
    pub fn rebuild_from_mirror(&mut self, module_id: &str) -> Result<ModuleRebuild> {
        ensure_id(module_id)?;
        let Some(mirror) = self.read_mirror(module_id)? else {
            return Err(Error::invalid_with(
                codes::MODULE_MIRROR_MISSING,
                [("module_id", module_id.to_string())],
            ));
        };
        let existing = self.tables_of(module_id)?;
        if !existing.is_empty() && self.isolation(module_id)?.is_none() {
            return Err(Error::invalid_with(
                codes::MODULE_DATA_REBUILD_LOCKED,
                [("module_id", module_id.to_string())],
            ));
        }

        // 先把建表语句全部备好并校验：任何一条不合规，都在**动库之前**拒。
        let mut plans = Vec::with_capacity(mirror.tables.len());
        let mut rows_total: i64 = 0;
        for table in &mirror.tables {
            plans.push(create_table_sql(table)?);
            rows_total += table.rows.len() as i64;
        }

        let tx = self.conn.transaction()?;
        for name in &existing {
            tx.execute(&format!("DROP TABLE IF EXISTS {}", quote(name)), []).map_err(|error| {
                Error::invalid_with(
                    codes::MODULE_MIRROR_FAILED,
                    [("detail", error.to_string())],
                )
            })?;
        }
        for (sql, table) in plans.iter().zip(&mirror.tables) {
            tx.execute(sql, []).map_err(|error| {
                Error::invalid_with(codes::MODULE_MIRROR_FAILED, [("detail", error.to_string())])
            })?;
            if table.rows.is_empty() {
                continue;
            }
            let placeholders = vec!["?"; table.columns.len()].join(", ");
            let insert = format!("INSERT INTO {} VALUES ({placeholders})", quote(&table.name));
            for row in &table.rows {
                let values = row.iter().map(decode_cell).collect::<Result<Vec<_>>>()?;
                tx.execute(&insert, rusqlite::params_from_iter(values)).map_err(|error| {
                    Error::invalid_with(
                        codes::MODULE_MIRROR_FAILED,
                        [("detail", error.to_string())],
                    )
                })?;
            }
        }
        if let Some(version) = mirror.schema_version {
            Self::record_schema_in(&tx, module_id, version)?;
        }
        tx.commit()?;

        Ok(ModuleRebuild {
            module_id: module_id.to_string(),
            tables: mirror.tables.len(),
            rows: rows_total,
            schema_version: mirror.schema_version,
        })
    }
}

/// 照着镜像里的列声明拼一条 `CREATE TABLE`（名字一律加引号；类型只认白名单形状）。
fn create_table_sql(table: &MirrorTable) -> Result<String> {
    let mut primary: Vec<(i64, &str)> = table
        .columns
        .iter()
        .filter(|column| column.pk > 0)
        .map(|column| (column.pk, column.name.as_str()))
        .collect();
    primary.sort_by_key(|(order, _)| *order);
    let composite = primary.len() > 1;

    let mut defs = Vec::with_capacity(table.columns.len() + 1);
    for column in &table.columns {
        defs.push(column_sql(column, primary.len() == 1)?);
    }
    if composite {
        let names = primary.iter().map(|(_, name)| quote(name)).collect::<Vec<_>>().join(", ");
        defs.push(format!("PRIMARY KEY ({names})"));
    }
    Ok(format!("CREATE TABLE {} ({})", quote(&table.name), defs.join(", ")))
}

fn column_sql(column: &MirrorColumn, inline_primary: bool) -> Result<String> {
    if column.name.is_empty() || column.name.chars().count() > 128 {
        return Err(Error::invalid_with(
            codes::MODULE_MIRROR_INVALID,
            [("detail", format!("bad column name in {}", column.name))],
        ));
    }
    let mut sql = quote(&column.name);
    if !column.kind.is_empty() {
        if !safe_kind(&column.kind) {
            return Err(Error::invalid_with(
                codes::MODULE_MIRROR_INVALID,
                [("detail", format!("bad column type: {}", column.kind))],
            ));
        }
        sql.push(' ');
        sql.push_str(&column.kind);
    }
    if column.not_null {
        sql.push_str(" NOT NULL");
    }
    if inline_primary && column.pk > 0 {
        sql.push_str(" PRIMARY KEY");
    }
    Ok(sql)
}

/// 声明类型只认这种形状：字母开头、字母数字下划线空格与括号逗号、≤32 字符。
///
/// 这一段会被拼进 SQL——镜像文件在磁盘上，可能被人改过；放任意文本进去等于开一道注入门。
fn safe_kind(kind: &str) -> bool {
    kind.len() <= 32
        && kind.chars().next().is_some_and(|first| first.is_ascii_alphabetic())
        && kind
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ' ' || ch == '(' || ch == ')' || ch == ',')
}
