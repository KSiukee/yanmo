//! 搬家：把整个数据目录**复制**到新位置，复制完按备份体检的**同一口径**核对一遍。
//!
//! # 为什么是复制，不是移动
//!
//! 改位置是"作者要换个地方放稿子"，不是"帮作者省几 MB 磁盘"。移动（复制成功就删旧）
//! 一旦哪一步误判，删掉的就是唯一一份稿子；复制最坏的结果只是多占一份空间。
//! 所以：**复制 + 核对 → 交回成功 → 旧位置原样留着**，删不删由作者自己定。
//!
//! # 为什么核对要借道 `Probe`
//!
//! 核对就是"把两份库按同一条 SQL 算一遍账"（[`work_stamps`]）。直接打开新位置那份会
//! 给它生成 `-wal` / `-shm`、甚至跑迁移——那等于**在核对的时候改动了被核对的东西**。
//! 所以两边都走 [`Probe`]：临时副本上算账，正本一字不动。

use std::path::{Path, PathBuf};

use super::backup::{last_write_at, work_stamps, Probe};
use crate::error::{Error, Result};
use crate::error_codes::codes;
use crate::location;
use crate::paths;

/// 搬家的结果：搬到哪、复制了多少（**旧位置没动过**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocation {
    pub dir: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

/// 把整个数据目录复制到 `to`。
///
/// 三条拒绝（都是"宁可不动，也别把稿子搞乱"）：原目录里没有库、新位置在老位置里面、
/// 新位置里已经有一份库。
pub fn copy_dir(from: &Path, to: &Path) -> Result<Relocation> {
    if !location::has_database(from) {
        return Err(Error::invalid_with(
            codes::STORE_RELOCATE_SOURCE_MISSING,
            [("path", from.display().to_string())],
        ));
    }
    if location::under(to, from) {
        return Err(Error::invalid_with(
            codes::STORE_RELOCATE_INSIDE,
            [("from", from.display().to_string()), ("to", to.display().to_string())],
        ));
    }
    if location::has_database(to) {
        return Err(Error::invalid_with(
            codes::STORE_RELOCATE_TARGET_IN_USE,
            [("path", to.display().to_string())],
        ));
    }
    std::fs::create_dir_all(to).map_err(|error| copy_failed(to, error))?;

    let mut report = Relocation { dir: to.to_path_buf(), files: 0, bytes: 0 };
    let mut stack = vec![from.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).map_err(|error| copy_failed(&current, error))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let target = to.join(path.strip_prefix(from).unwrap_or(&path));
            let kind = entry.file_type().map_err(|error| copy_failed(&path, error))?;
            // 符号链接一概不跟：跟出去可能把"别的地方"卷进来，也可能成环
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                std::fs::create_dir_all(&target).map_err(|error| copy_failed(&target, error))?;
                stack.push(path);
                continue;
            }
            let bytes = std::fs::copy(&path, &target).map_err(|error| copy_failed(&path, error))?;
            report.files += 1;
            report.bytes += bytes;
        }
    }
    Ok(report)
}

/// 核对：新位置与老位置**逐书规模账完全一致**才算成（口径与备份体检同一条 SQL）。
pub fn verify_same_scale(new_dir: &Path, source_dir: &Path) -> Result<()> {
    let new_db = new_dir.join(paths::DB_FILE);
    let source_db = source_dir.join(paths::DB_FILE);
    let (same, detail) = match (Probe::open(&new_db), Probe::open(&source_db)) {
        (Ok(new), Ok(old)) => {
            let left = work_stamps(new.conn()).ok();
            let right = work_stamps(old.conn()).ok();
            let stamps = left.is_some() && left == right;
            let written = last_write_at(new.conn()).ok() == last_write_at(old.conn()).ok();
            (stamps && written, "scale mismatch between the copy and the original".to_string())
        }
        (Err(reason), _) => (false, format!("the copy cannot be opened: {reason}")),
        (_, Err(reason)) => (false, format!("the original cannot be opened: {reason}")),
    };
    if same {
        return Ok(());
    }
    Err(Error::invalid_with(
        codes::STORE_RELOCATE_VERIFY_FAILED,
        [("path", new_dir.display().to_string()), ("detail", detail)],
    ))
}

/// 复制失败：把出错的路径与底层原因一起交出去（界面才知道是哪一步、为什么）。
fn copy_failed(path: &Path, error: std::io::Error) -> Error {
    Error::invalid_with(
        codes::STORE_RELOCATE_COPY_FAILED,
        [("path", path.display().to_string()), ("detail", error.to_string())],
    )
}
