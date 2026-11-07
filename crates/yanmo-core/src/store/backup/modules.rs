// i18n-allow-file: 本文件不产界面文案——体检问题只给「码 + 参数」（见 error_codes.rs），
// 剩下的中文只在注释与日志模板里。

//! 备份里的**模块数据**：`modules.db` 的快照、清单戳与体检。
//!
//! 单独成件的原因：核心库快照与模块库快照是两件事——前者是稿子，后者是模块的数据区；
//! 它们的失败口径与体检项不同，混在 `backup.rs` 里会把那条已经够长的备份链再撑大。
//!
//! **读**永远用只读连接：备份不该改动源库（连建一张账表都不该）。文件不存在就是
//! "这台机器还没装过模块"，不是错误。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use super::MODULES_FILE;
use crate::error::{codes, Diagnosis, Error, Result};

/// 一份备份里带的模块数据戳（`modules.db` 的快照）。
///
/// `schemas` 是每个模块的数据结构版本（`module_schema` 表）：体检时拿它跟快照里的
/// 那一份对一遍——对不上说明这份模块快照不是"那个时刻"的，不能当完整备份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleBackupStamp {
    /// 快照在包里的文件名（现在恒为 `modules.db`；留字段便于将来换名/多文件）
    pub file: String,
    pub bytes: u64,
    /// 模块 id → 数据结构版本
    pub schemas: BTreeMap<String, i64>,
}

/// 把数据目录里的 `modules.db` 做成一份一致性快照，放进暂存区。
///
/// 文件不存在 = 没装过模块，返回 `None`（不是错误）；有文件但读不出来 = 这次备份
/// 做不出来（**不**悄悄退化成"只备份稿子"——那会让作者以为模块数据也在包里）。
pub(super) fn snapshot_modules(
    data_dir: &Path,
    staging: &Path,
) -> Result<Option<ModuleBackupStamp>> {
    let source = data_dir.join(MODULES_FILE);
    if !source.is_file() {
        return Ok(None);
    }
    let target = staging.join(MODULES_FILE);
    let conn = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| module_snapshot_failed(format!("打不开模块数据：{error}")))?;
    conn.execute("VACUUM INTO ?1", params![target.to_string_lossy().to_string()])
        .map_err(|error| module_snapshot_failed(format!("模块数据快照写不出来：{error}")))?;
    let bytes = std::fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
    let schemas = module_schema_versions(&conn)
        .map_err(|error| module_snapshot_failed(format!("读不出模块数据结构版本：{error}")))?;
    Ok(Some(ModuleBackupStamp { file: MODULES_FILE.to_string(), bytes, schemas }))
}

/// 体检包里的模块快照：在不在、大小、结构、版本账。
pub(super) fn verify_modules(package: &Path, stamp: &ModuleBackupStamp) -> Vec<Diagnosis> {
    let mut problems = Vec::new();
    if stamp.file != MODULES_FILE {
        problems.push(Diagnosis::with(
            codes::BACKUP_MODULES_NAME_MISMATCH,
            [("file", stamp.file.clone())],
        ));
        return problems;
    }
    let modules = package.join(MODULES_FILE);
    if !modules.is_file() {
        problems.push(Diagnosis::with(
            codes::BACKUP_MODULES_MISSING,
            [("file", MODULES_FILE.to_string())],
        ));
        return problems;
    }
    match std::fs::metadata(&modules) {
        Ok(meta) if meta.len() == stamp.bytes => {}
        Ok(meta) => problems.push(Diagnosis::with(
            codes::BACKUP_MODULES_SIZE_MISMATCH,
            [("listed", stamp.bytes.to_string()), ("found", meta.len().to_string())],
        )),
        Err(error) => problems.push(Diagnosis::with(
            codes::BACKUP_MODULES_UNREADABLE,
            [("detail", error.to_string())],
        )),
    }
    match super::Probe::open(&modules) {
        Err(problem) => problems.push(problem),
        Ok(probe) => {
            match crate::db::quick_check(probe.conn()) {
                Ok(verdict) if verdict == "ok" => {}
                Ok(verdict) => problems.push(Diagnosis::with(
                    codes::BACKUP_MODULES_BROKEN,
                    [("verdict", verdict)],
                )),
                Err(error) => problems.push(Diagnosis::with(
                    codes::BACKUP_MODULES_CHECK_FAILED,
                    [("detail", error.to_string())],
                )),
            }
            match module_schema_versions(probe.conn()) {
                Ok(schemas) if schemas == stamp.schemas => {}
                Ok(_) => problems.push(Diagnosis::with(
                    codes::BACKUP_MODULES_SCHEMA_MISMATCH,
                    [],
                )),
                Err(error) => problems.push(Diagnosis::with(
                    codes::BACKUP_MODULES_SCHEMA_UNREADABLE,
                    [("detail", error.to_string())],
                )),
            }
        }
    }
    problems
}

/// 读 `modules.db` 里的 `module_schema`（没这张表就是空的——老库还没记过账）。
fn module_schema_versions(conn: &Connection) -> Result<BTreeMap<String, i64>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'module_schema')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(BTreeMap::new());
    }
    let mut stmt =
        conn.prepare("SELECT module_id, schema_version FROM module_schema ORDER BY module_id")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (module_id, version) = row?;
        out.insert(module_id, version);
    }
    Ok(out)
}

fn module_snapshot_failed(detail: String) -> Error {
    Error::invalid_with(codes::MODULE_SNAPSHOT_FAILED, [("detail", detail)])
}
