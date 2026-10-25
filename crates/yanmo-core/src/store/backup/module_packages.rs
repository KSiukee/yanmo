//! 备份里的**模块程序包**：把受管 `modules/` 目录整体复制进备份包。
//!
// i18n-allow-file: 备份诊断由核心生成、界面原样展示（与 backup.rs 同一口径）。
//! 只做普通文件；遇到符号链接直接失败（模块包本来就不该有）。指纹按
//! `相对路径\\0文件字节` 的排序拼接后取 SHA-256，恢复前可逐字节核对。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{codes, Error, Result};

pub const DIR: &str = "module-packages";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModulePackagesStamp {
    pub files: u64,
    pub bytes: u64,
    pub sha256: String,
}

pub(super) fn snapshot(data_dir: &Path, staging: &Path) -> Result<Option<ModulePackagesStamp>> {
    let source = data_dir.join("modules");
    if !source.is_dir() {
        return Ok(None);
    }
    let target = staging.join(DIR);
    copy_tree(&source, &target)?;
    Ok(Some(measure(&target)?))
}

pub(super) fn verify(package: &Path, stamp: &ModulePackagesStamp) -> Vec<String> {
    let dir = package.join(DIR);
    if !dir.is_dir() {
        return vec![format!("找不到模块程序包目录：{DIR}")];
    }
    match measure(&dir) {
        Ok(actual) if actual == *stamp => Vec::new(),
        Ok(actual) => vec![format!(
            "模块程序包对不上：清单 {} 文件/{} 字节/{}，包里 {} 文件/{} 字节/{}",
            stamp.files, stamp.bytes, stamp.sha256, actual.files, actual.bytes, actual.sha256
        )],
        Err(error) => vec![format!("模块程序包读不出来：{error}")],
    }
}

/// 把备份包里的程序目录复制到数据目录；已有目录先留底到 `旧库留底/<stamp>/modules`。
pub(super) fn restore(source: &Path, data_dir: &Path, stamp: &str) -> Result<bool> {
    let from = source.join(DIR);
    if !from.is_dir() {
        return Ok(false);
    }
    let staging = data_dir.join("modules.restoring");
    let target = data_dir.join("modules");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).ok();
    }
    copy_tree(&from, &staging)
        .map_err(|error| Error::invalid_with(codes::BACKUP_RESTORE_SWAP_FAILED, [("detail", format!("复制模块程序包失败：{error}"))]))?;
    if target.exists() {
        let keep = data_dir.join(crate::store::restore_swap::KEEP_FOLDER).join(stamp);
        std::fs::create_dir_all(&keep)?;
        let old = keep.join("modules");
        if old.exists() {
            std::fs::remove_dir_all(&old).ok();
        }
        std::fs::rename(&target, &old).map_err(|error| {
            Error::invalid_with(
                codes::BACKUP_RESTORE_SWAP_FAILED,
                [("detail", format!("留底旧模块程序包失败：{error}"))],
            )
        })?;
    }
    if let Err(error) = std::fs::rename(&staging, &target) {
        std::fs::remove_dir_all(&staging).ok();
        return Err(Error::invalid_with(
            codes::BACKUP_RESTORE_SWAP_FAILED,
            [("detail", format!("放回模块程序包失败：{error}"))],
        ));
    }
    Ok(true)
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    let mut files = BTreeMap::new();
    collect(source, source, &mut files)?;
    std::fs::create_dir_all(target)?;
    for (relative, path) in files {
        let destination = target.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(path, destination)?;
    }
    Ok(())
}

fn measure(root: &Path) -> Result<ModulePackagesStamp> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    for (relative, path) in &files {
        let content = std::fs::read(path)?;
        hasher.update(relative.as_os_str().to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(&content);
        bytes = bytes.saturating_add(content.len() as u64);
    }
    Ok(ModulePackagesStamp {
        files: files.len() as u64,
        bytes,
        sha256: hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect(),
    })
}

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_symlink() {
            return Err(Error::invalid_with(
                codes::BACKUP_SNAPSHOT_FAILED,
                [("detail", "模块程序包里发现符号链接".to_string())],
            ));
        }
        let meta = entry.metadata()?;
        if meta.is_dir() {
            collect(root, &path, files)?;
        } else if meta.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::invalid(codes::BACKUP_SNAPSHOT_FAILED))?
                .to_path_buf();
            files.insert(relative, path);
        }
    }
    Ok(())
}
