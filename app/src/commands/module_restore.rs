//! 整库恢复：**一键**把一份镜像目录放回新库。
//!
//! 以前是手工三步（建空库 → 拷 `module-mirrors/` → 逐个模块点重建）。现在只剩
//! 一步：选一个备份目录，壳把镜像拷进当前数据目录并逐模块重建。
//!
//! 三条纪律：
//! 1. **只认镜像文件**：备份目录里没有 `module-mirrors/` 就找它自己那一层的 `*.json`；
//! 2. **逐模块、各自成败**：一个模块恢复失败不挡其他模块（每个结果都带原因）；
//! 3. **不覆盖活数据**：库里还有表且这个模块没被隔离 → 拒（核心那一道门，壳不绕过）。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;
use yanmo_core::store::{ModuleData, MIRROR_DIR};

use crate::error::ApiError;
use crate::storage::AppData;

/// 一个模块的恢复结果。
#[derive(Debug, Clone, Serialize)]
pub struct RestoreItem {
    pub module_id: String,
    pub ok: bool,
    pub tables: usize,
    pub rows: i64,
    /// 失败时的稳定码 / 说明
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    /// 找到多少份镜像
    pub found: usize,
    pub items: Vec<RestoreItem>,
}

/// 整库恢复：`from` 可以是**数据目录的备份**（里面有 `module-mirrors/`），
/// 也可以直接是那份 `module-mirrors/` 目录。
#[tauri::command(rename_all = "snake_case")]
pub fn module_restore_all(
    data: State<'_, AppData>,
    from: String,
) -> Result<RestoreReport, ApiError> {
    let source = resolve_mirror_dir(Path::new(&from))
        .ok_or_else(|| ApiError::new("shell.module_restore_no_mirrors"))?;
    let live = data.data_dir().join("yanmo.db");
    let live = ModuleData::path_beside(&live);
    let target_dir = live.join(MIRROR_DIR);
    fs::create_dir_all(&target_dir)
        .map_err(|error| ApiError::new("shell.module_restore_failed").caused_by(error))?;

    let mut ids: Vec<String> = fs::read_dir(&source)
        .map_err(|error| ApiError::new("shell.module_restore_failed").caused_by(error))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    let found = ids.len();

    let mut module_data = ModuleData::open(&live)?;
    let mut items = Vec::with_capacity(found);
    for module_id in ids {
        // 先把镜像搬到"活数据目录"里（重建只认这里的那一份）
        let copy = fs::copy(source.join(format!("{module_id}.json")), target_dir.join(format!("{module_id}.json")));
        if let Err(error) = copy {
            items.push(RestoreItem {
                module_id,
                ok: false,
                tables: 0,
                rows: 0,
                detail: format!("copy: {error}"),
            });
            continue;
        }
        match module_data.rebuild_from_mirror(&module_id) {
            Ok(rebuilt) => items.push(RestoreItem {
                module_id,
                ok: true,
                tables: rebuilt.tables,
                rows: rebuilt.rows,
                detail: String::new(),
            }),
            Err(error) => items.push(RestoreItem {
                module_id,
                ok: false,
                tables: 0,
                rows: 0,
                detail: format!("{error}"),
            }),
        }
    }
    Ok(RestoreReport { found, items })
}

/// 选一个备份目录（给「整库恢复」用；选完不了什么，只回路径）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_restore_pick(title: String) -> Option<String> {
    crate::pick_dir::pick(&title).map(|path| path.to_string_lossy().to_string())
}

/// 备份目录 → 真正放镜像的那一层。
fn resolve_mirror_dir(from: &Path) -> Option<PathBuf> {
    if !from.is_dir() {
        return None;
    }
    let nested = from.join(MIRROR_DIR);
    if nested.is_dir() {
        return Some(nested);
    }
    let has_json = fs::read_dir(from)
        .ok()?
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"));
    has_json.then(|| from.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_may_be_the_data_dir_or_the_mirror_dir_itself() {
        let dir = tempfile::tempdir().unwrap();
        let mirrors = dir.path().join(MIRROR_DIR);
        fs::create_dir(&mirrors).unwrap();
        fs::write(mirrors.join("app.a.json"), b"{}").unwrap();
        assert_eq!(resolve_mirror_dir(dir.path()), Some(mirrors.clone()));
        assert_eq!(resolve_mirror_dir(&mirrors), Some(mirrors));
        assert!(resolve_mirror_dir(&dir.path().join("nope")).is_none());
    }
}
