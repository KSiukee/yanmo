//! 从备份恢复（二）：**列得出哪些备份能恢复**。
//!
//! 只看清单、不体检：体检要把整份库复制出来跑一遍，列表里对每一份都做一遍太贵
//! （作者的备份盘上可能躺着几十份）。真正要看的那一份，选中时再体检（见 [`super::restore`]）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::backup::{read_manifest, PACKAGE_PREFIX};
use super::backup_seal::is_sealed_package;

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
    /// 加密包（`.ympkg` 那一个文件）：**清单在密文里面**，所以上面那些数字这里给不出来，
    /// 只能从文件名与文件本身认出「有这么一份、什么时候写的、多大」——要看清内容，
    /// 得先给钥匙解开（解开之后就是一份标准包目录，走既有那条体检/换库链路）。
    #[serde(default)]
    pub sealed: bool,
}

/// 扫一个目录里的备份包（**只认自己写出来的**：名字前缀 + 包里有我们的清单）。
///
/// 两种形态都算：**明文包是目录**，**加密包是一个 `.ympkg` 文件**。
/// 加密包读不到清单（那正是它的意义），所以只按名字与文件的元信息给个摘要。
pub fn list_packages(root: &Path) -> Vec<PackageBrief> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<PackageBrief> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with(PACKAGE_PREFIX))
                .unwrap_or(false)
        })
        .filter_map(|path| {
            if path.is_dir() {
                package_brief(&path)
            } else if is_sealed_package(&path) {
                Some(sealed_brief(&path))
            } else {
                None
            }
        })
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
        sealed: false,
    })
}

/// 加密包的摘要：清单在密文里，所以**只有名字与文件本身**这三样是能看见的。
///
/// - `stamp`：从文件名里抠出来（`研墨备份-20260929-1830.ympkg` → `20260929-1830`），
///   与明文包用同一套排序口径；
/// - `created_at`：文件最后写入的时间（**这是诚实的近似**：它就是"这份备份写下去的那一刻"）；
/// - `bytes`：文件大小（明文包那边是整个目录的字节数）。
/// 其余字段一律留空值：要看清内容，得先给钥匙解开——**不猜、不假装知道**。
fn sealed_brief(path: &Path) -> PackageBrief {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stamp = name
        .trim_start_matches(PACKAGE_PREFIX)
        .trim_end_matches(&format!(".{}", super::super::seal::SEAL_EXT))
        .to_string();
    let metadata = std::fs::metadata(path).ok();
    let created_at = metadata
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0);
    PackageBrief {
        path: path.to_string_lossy().to_string(),
        stamp,
        created_at,
        device: String::new(),
        works: 0,
        chapters: 0,
        words: 0,
        bytes: metadata.map(|m| m.len()).unwrap_or(0),
        data_format_version: 0,
        sealed: true,
    }
}
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
