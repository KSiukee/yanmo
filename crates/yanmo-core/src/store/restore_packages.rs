//! 从备份恢复（二）：**列得出哪些备份能恢复**。
//!
//! 只看清单、不体检：体检要把整份库复制出来跑一遍，列表里对每一份都做一遍太贵
//! （作者的备份盘上可能躺着几十份）。真正要看的那一份，选中时再体检（见 [`super::restore`]）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::backup::{read_manifest, PACKAGE_PREFIX};

/// 一份备份包的摘要（列表用：只看清单，不体检——体检要复制整份库，太贵）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageBrief {
    pub path: String,
    pub stamp: String,
    pub created_at: i64,
    pub device: String,
    pub works: i64,
    pub chapters: i64,
    pub words: i64,
    pub bytes: u64,
    pub data_format_version: u32,
}

/// 扫一个目录里的备份包（**只认自己写出来的**：目录名前缀 + 包里有我们的清单）。
pub fn list_packages(root: &Path) -> Vec<PackageBrief> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<PackageBrief> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .map(|name| name.to_string_lossy().starts_with(PACKAGE_PREFIX))
                    .unwrap_or(false)
        })
        .filter_map(|path| package_brief(&path))
        .collect();
    sort_newest_first(&mut out);
    out
}

/// 扫若干目录（作者的每个备份位置），**按路径去重**后从新到旧。
pub fn scan_packages(roots: &[PathBuf]) -> Vec<PackageBrief> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for root in roots {
        for brief in list_packages(root) {
            if seen.insert(brief.path.clone()) {
                out.push(brief);
            }
        }
    }
    sort_newest_first(&mut out);
    out
}

fn sort_newest_first(packages: &mut [PackageBrief]) {
    // 名字里带 `YYYYMMDD-HHMM`，字面量从新到旧排序就是时间排序
    packages.sort_by(|a, b| b.stamp.cmp(&a.stamp).then_with(|| b.created_at.cmp(&a.created_at)));
}

fn package_brief(package: &Path) -> Option<PackageBrief> {
    let manifest = read_manifest(package)?;
    let mut chapters = 0;
    let mut words = 0;
    for work in &manifest.works {
        chapters += work.chapters;
        words += work.word_count;
    }
    Some(PackageBrief {
        path: package.to_string_lossy().to_string(),
        stamp: manifest.stamp,
        created_at: manifest.created_at,
        device: manifest.device,
        works: manifest.works.len() as i64,
        chapters,
        words,
        bytes: dir_bytes(package),
        data_format_version: manifest.data_format_version,
    })
}

/// 一份备份"实质内容"的字节数（整包目录，含成稿）——列表里给作者看大小。
pub(super) fn dir_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(_) => {
                    total = total.saturating_add(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));
                }
                Err(_) => {}
            }
        }
    }
    total
}
