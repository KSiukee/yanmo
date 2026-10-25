//! 按模块导出数据：只把 `mod_<id>_*` 的表复制到一个新库，不把别人的数据带走。
//!
//! 复制用 SQLite `ATTACH` + `CREATE TABLE ... AS SELECT`；**表结构/索引不会逐字保留**，
//! 导出的是数据和列。整库一致性快照仍用 `snapshot_into`。

use std::path::Path;

use super::module_data::{quote, ModuleData};
use crate::error::{codes, Error, Result};

impl ModuleData {
    pub fn export_namespace(&self, module_id: &str, target: impl AsRef<Path>) -> Result<()> {
        super::module_data::ensure_id(module_id)?;
        let target = target.as_ref();
        if target.exists() {
            return Err(Error::invalid_with(
                codes::MODULE_SNAPSHOT_FAILED,
                [("detail", format!("target exists: {}", target.display()))],
            ));
        }
        let target_text = target.to_string_lossy().to_string();
        self.conn
            .execute("ATTACH DATABASE ?1 AS exported", [&target_text])
            .map_err(|error| {
                Error::invalid_with(codes::MODULE_SNAPSHOT_FAILED, [("detail", error.to_string())])
            })?;
        let copied = (|| -> Result<()> {
            for table in self.tables_of(module_id)? {
                self.conn
                    .execute(
                        &format!(
                            "CREATE TABLE exported.{} AS SELECT * FROM main.{}",
                            quote(&table),
                            quote(&table)
                        ),
                        [],
                    )
                    .map_err(|error| {
                        Error::invalid_with(
                            codes::MODULE_SNAPSHOT_FAILED,
                            [("detail", error.to_string())],
                        )
                    })?;
            }
            self.conn
                .execute(
                    "CREATE TABLE exported.module_schema (
                        module_id      TEXT PRIMARY KEY,
                        schema_version INTEGER NOT NULL,
                        updated_at     INTEGER NOT NULL
                    )",
                    [],
                )
                .map_err(|error| {
                    Error::invalid_with(codes::MODULE_SNAPSHOT_FAILED, [("detail", error.to_string())])
                })?;
            self.conn
                .execute(
                    "INSERT INTO exported.module_schema
                     SELECT * FROM main.module_schema WHERE module_id = ?1",
                    [module_id],
                )
                .map_err(|error| {
                    Error::invalid_with(codes::MODULE_SNAPSHOT_FAILED, [("detail", error.to_string())])
                })?;
            Ok(())
        })();
        let _ = self.conn.execute("DETACH DATABASE exported", []);
        copied
    }
}
