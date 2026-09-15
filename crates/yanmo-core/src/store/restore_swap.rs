//! 从备份恢复（三）：**真正换库**——最危险的十几步单独放在这里，便于一处盯紧。
//!
//! 顺序是刻意的，别调换：
//!
//! 1. 把要恢复的那一份复制到**临时名**（绝不搬包：备份包是被恢复的对象，不是消耗品）；
//! 2. 原库（`.db` + `-wal` + `-shm`）整体**留底**——改名而不是删，坏库也留证；
//! 3. 临时名 `rename` 成正式名（同一目录内，不会出现"半个库"）；
//! 4. 当场再体检一次：不过就把它扔掉、把留底搬回来——**失败必须能回到原状**。
//!
//! 调用方必须**先关库**（连接还开着就动文件，是拿作者的稿子冒险）。这里不认识"谁开着库"。

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::backup::read_manifest;
use super::restore::source_invalid;
use crate::error::{codes, Error, Result};
use crate::paths::DB_FILE;

/// 换库前那一份放这儿（**改名而不是删**：坏库也要留证，作者还能自己捞）。
pub const KEEP_FOLDER: &str = "旧库留底";
/// 换库时的临时名：先放稳，再在同一个目录里 `rename` 过去（不会出现"半个库"）。
const STAGING_FILE: &str = "yanmo.db.restoring";

/// 换库结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreOutcome {
    pub restored_from: String,
    /// 原库留底目录（原来就没有库时是空串）
    pub quarantine: String,
    pub bytes: u64,
    pub stamp: String,
}

/// 换库：**先把新库放稳，再把原库留底，最后当场体检一次**。
///
/// 失败时原库会被搬回原处（[`RestoreOutcome`] 只在成功时返回）。
pub fn swap_in(source: &Path, data_dir: &Path, stamp: &str) -> Result<RestoreOutcome> {
    let source_db = database_file(source)?;
    let target = data_dir.join(DB_FILE);
    if same_file(&source_db, &target) {
        return Err(source_invalid(source));
    }
    std::fs::create_dir_all(data_dir)?;

    // ① 复制到临时名（先把"要换上去的那一份"放稳，后续每一步都能退回来）
    let staged = data_dir.join(STAGING_FILE);
    std::fs::remove_file(&staged).ok();
    if let Err(error) = copy_synced(&source_db, &staged) {
        std::fs::remove_file(&staged).ok();
        return Err(swap_failed(format!("把备份里的库复制过来时失败：{error}")));
    }
    let bytes = std::fs::metadata(&staged).map(|meta| meta.len()).unwrap_or(0);

    // ② 原库整体留底（`.db` 与 `-wal`/`-shm` **必须一起走**：只搬 .db 会把不一致的 WAL 留给新库）
    let keep = data_dir.join(KEEP_FOLDER).join(stamp);
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut quarantine = String::new();
    for suffix in ["", "-wal", "-shm"] {
        let from = data_dir.join(format!("{DB_FILE}{suffix}"));
        if !from.exists() {
            continue;
        }
        if moved.is_empty() {
            if let Err(error) = std::fs::create_dir_all(&keep) {
                std::fs::remove_file(&staged).ok();
                return Err(swap_failed(format!("建留底目录失败：{error}")));
            }
            quarantine = keep.to_string_lossy().to_string();
        }
        let to = keep.join(format!("{DB_FILE}{suffix}"));
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push((from, to)),
            Err(error) => {
                let rollback_failed = rollback(&moved);
                std::fs::remove_file(&staged).ok();
                return Err(swap_failed_after_rollback(
                    format!("把原库搬去留底时失败：{error}"),
                    rollback_failed,
                ));
            }
        }
    }

    // ③ 放新库（同一目录内 rename，不会出现"半个库"）
    if let Err(error) = std::fs::rename(&staged, &target) {
        let rollback_failed = rollback(&moved);
        std::fs::remove_file(&staged).ok();
        return Err(swap_failed_after_rollback(
            format!("把新库放回数据目录时失败：{error}"),
            rollback_failed,
        ));
    }

    // ④ 当场再体检一次：不过就把它扔掉、把留底搬回来——**失败必须能回到原状**
    match quick_check_file(&target) {
        Ok(()) => Ok(RestoreOutcome {
            restored_from: source_db.to_string_lossy().to_string(),
            quarantine,
            bytes,
            stamp: stamp.to_string(),
        }),
        Err(problem) => {
            std::fs::remove_file(&target).ok();
            for suffix in ["-wal", "-shm"] {
                std::fs::remove_file(data_dir.join(format!("{DB_FILE}{suffix}"))).ok();
            }
            Err(swap_failed_after_rollback(problem, rollback(&moved)))
        }
    }
}

/// 来源里那个真正的库文件：备份包认清单指的 `yanmo.db`，裸库就是它自己。
fn database_file(source: &Path) -> Result<PathBuf> {
    if source.is_dir() {
        let manifest = read_manifest(source).ok_or_else(|| source_invalid(source))?;
        let db = source.join(&manifest.snapshot_file);
        return if db.is_file() { Ok(db) } else { Err(source_invalid(source)) };
    }
    if source.is_file() {
        return Ok(source.to_path_buf());
    }
    Err(source_invalid(source))
}

/// 把留底搬回原处，并收掉空掉的留底目录。
fn rollback(moved: &[(PathBuf, PathBuf)]) -> Vec<String> {
    let mut failed = Vec::new();
    for (original, kept) in moved {
        if let Err(error) = std::fs::rename(kept, original) {
            failed.push(format!("把 {} 搬回 {} 失败：{error}", kept.display(), original.display()));
        }
    }
    if failed.is_empty() {
        if let Some((_, first)) = moved.first() {
            if let Some(dir) = first.parent() {
                let _ = std::fs::remove_dir(dir);
            }
        }
    }
    failed
}

/// 换库失败的错误：**回滚也失败时把"真库在哪"写进 detail**，不然作者只看到一句"换库失败"。
///
/// 2026-09-15 代码质量评审：中等 3——回滚失败以前被 `let _ =` 吞掉，壳层失败分支接着
/// 去 `reopen_store()`，那一步会在原位建出一个空库，界面于是若无其事地跑在空书架上。
fn swap_failed_after_rollback(problem: String, rollback_failed: Vec<String>) -> Error {
    if rollback_failed.is_empty() {
        return swap_failed(problem);
    }
    swap_failed(format!(
        "{problem}；**而且回滚也失败了**（真库可能只剩在「旧库留底」目录里）：{}",
        rollback_failed.join("；")
    ))
}

/// 复制 + 落盘（`fs::copy` 不保证内容已经落到介质上，换库这一步必须稳）。
///
/// `sync_all` 要**写权限的句柄**：Windows 上拿只读句柄刷盘会直接"拒绝访问"。
fn copy_synced(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::copy(from, to)?;
    std::fs::OpenOptions::new().write(true).open(to)?.sync_all()
}

/// 新库放下之后当场体检：结构完好才算换成功。
fn quick_check_file(db: &Path) -> std::result::Result<(), String> {
    let conn = Connection::open(db).map_err(|error| format!("换上去的库打不开：{error}"))?;
    match crate::db::quick_check(&conn) {
        Ok(verdict) if verdict == "ok" => Ok(()),
        Ok(verdict) => Err(format!("换上去的库结构有问题：{verdict}")),
        Err(error) => Err(format!("换上去的库体检失败：{error}")),
    }
}

/// 两个路径指不指同一个文件（换库时用来拦住"拿活库恢复活库"）。
pub(super) fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => a == b,
    }
}

fn swap_failed(detail: String) -> Error {
    Error::invalid_with(codes::BACKUP_RESTORE_SWAP_FAILED, [("detail", detail)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **回滚失败必须被说出来**（2026-09-15 代码质量评审：中等 3）。
    ///
    /// 一对不存在的路径就能让 `rename` 失败——以前这里被 `let _ =` 吞掉，上层只知道
    /// "换库失败"，不知道真库只剩在留底目录里。现在必须留下记录，并且带上去哪找。
    #[test]
    fn a_failed_rollback_is_reported_not_swallowed() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("原位").join("yanmo.db");
        let kept = dir.path().join("留底").join("yanmo.db"); // 不存在 → 搬不回去

        let failed = rollback(&[(original, kept)]);
        assert_eq!(failed.len(), 1, "搬不回去必须留一条记录，不能静默吞掉");
        assert!(failed[0].contains("留底"), "记录里要带留底路径，作者才知道去哪找：{}", failed[0]);

        // 错误文案里必须出现「回滚也失败」——不然作者只看到一句"换库失败"，会继续写下去
        let error = swap_failed_after_rollback("换库失败".to_string(), failed);
        let text = format!("{error:?}");
        assert!(text.contains("回滚也失败"), "{text}");
        assert!(text.contains("留底"), "{text}");
    }

    /// 回滚干净时错误文案保持原样，别把作者吓一跳。
    #[test]
    fn a_clean_rollback_keeps_the_plain_error() {
        let error = swap_failed_after_rollback("换库失败".to_string(), Vec::new());
        assert!(!format!("{error:?}").contains("回滚也失败"), "干净的失败不该牵扯回滚");
    }
}
