// i18n-allow-file: 备份体检的诊断清单（给测试与日志看，不直接进界面；与 backup.rs 同一条口径）
//! 备份包的两样"说明书"：`成稿/目录.json`（显式目录索引）与 `settings.json`（设置快照）。
//!
//! 它们**不是备份的主体**（主体仍是快照与成稿），而是给"人核对"与"程序重建"用的：
//!
//! - 目录索引把人能读的层级、顺序、字数、内容指纹**显式**写出来（`work.json` 的顺序是隐含的）；
//! - 设置快照只放**能安全回灌、且与作品/外观有关**的项——**绝不**放 device_id / 会话 / 路径；
//! - 两者都列进 manifest 的 `extras`（名字 / 字节 / SHA-256），体检时逐个核对；
//! - 老包没有这两样：`extras` 为空，照样体检通过（向后兼容）。

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::backup::{ExtraFile, WorkStamp};
use super::Store;
use crate::error::Result;
use crate::version;

/// 目录索引在包里的相对路径。
pub(super) const CATALOG_FILE: &str = "成稿/目录.json";
/// 设置快照在包里的相对路径。
pub(super) const SETTINGS_FILE: &str = "settings.json";
/// 目录索引的格式号（将来加字段靠它判新旧）。
const CATALOG_VERSION: u32 = 1;
/// 设置快照的格式号。
const SETTINGS_VERSION: u32 = 1;

#[derive(Serialize)]
struct CatalogNode {
    order: usize,
    depth: usize,
    kind: &'static str,
    title: String,
    word_count: i64,
    char_count: i64,
    chars_no_punct: i64,
    has_body: bool,
    empty: bool,
    /// 正文指纹（空章是空串）——"这一章的字有没有被换过"就看它
    content_hash: String,
}

#[derive(Serialize)]
struct CatalogWork {
    id: i64,
    title: String,
    language: &'static str,
    chapters: i64,
    word_count: i64,
    chars_no_punct: i64,
    fingerprint: String,
    nodes: Vec<CatalogNode>,
}

/// 写出目录索引与设置快照，返回它们进 manifest 的那份账。
pub(super) fn write_extras(
    store: &Store,
    staging: &Path,
    works: &[WorkStamp],
) -> Result<Vec<ExtraFile>> {
    let mut catalog_works = Vec::new();
    for work in works {
        let detail = store.get_work(work.id)?;
        let nodes = store.list_nodes(work.id)?;
        // 显式顺序：按 parent + sort_order 做先序遍历（数组顺序就是阅读顺序）
        let mut children: HashMap<Option<i64>, Vec<(i64, i64)>> = HashMap::new();
        for node in &nodes {
            children.entry(node.parent_id).or_default().push((node.sort_order, node.id));
        }
        for list in children.values_mut() {
            list.sort_unstable();
        }
        let by_id: HashMap<i64, &super::NodeSummary> =
            nodes.iter().map(|node| (node.id, node)).collect();
        let mut ordered = Vec::new();
        fn walk(
            parent: Option<i64>,
            depth: usize,
            children: &HashMap<Option<i64>, Vec<(i64, i64)>>,
            out: &mut Vec<(usize, usize, i64)>,
        ) {
            if let Some(list) = children.get(&parent) {
                for (_, id) in list {
                    out.push((out.len() + 1, depth, *id));
                    walk(Some(*id), depth + 1, children, out);
                }
            }
        }
        walk(None, 0, &children, &mut ordered);

        let mut catalog_nodes = Vec::new();
        for (order, depth, id) in ordered {
            let Some(node) = by_id.get(&id) else { continue };
            let content_hash = if node.has_body {
                store.body_fingerprint(id).unwrap_or_default()
            } else {
                String::new()
            };
            catalog_nodes.push(CatalogNode {
                order,
                depth,
                kind: node.kind.as_str(),
                title: node.title_rendered.clone(),
                word_count: node.word_count,
                char_count: node.char_count,
                chars_no_punct: node.chars_no_punct,
                has_body: node.has_body,
                empty: !node.has_body && node.word_count == 0,
                content_hash,
            });
        }
        catalog_works.push(CatalogWork {
            id: work.id,
            title: work.title.clone(),
            language: detail.language.as_str(),
            chapters: work.chapters,
            word_count: work.word_count,
            chars_no_punct: work.chars_no_punct,
            fingerprint: work.fingerprint.clone(),
            nodes: catalog_nodes,
        });
    }

    let catalog = serde_json::json!({
        "format": "yanmo-catalog",
        "version": CATALOG_VERSION,
        "engine_version": version::engine_version(),
        "data_format_version": version::DATA_FORMAT_VERSION,
        "works": catalog_works,
    });
    let catalog_bytes = serde_json::to_vec_pretty(&catalog).unwrap_or_default();

    // 设置快照：只收安全、可回灌的项
    let global = store.appearance(None)?;
    let mut settings_works = Vec::new();
    for work in works {
        let appearance = store.appearance(Some(work.id))?;
        settings_works.push(serde_json::json!({
            "id": work.id,
            "title": work.title,
            "language": store.get_work(work.id)?.language.as_str(),
            "volume_target": store.volume_target(work.id)?,
            "naming": appearance.naming.map(|naming| naming.as_str()),
            "chapter_numbering": appearance.chapter_numbering.as_str(),
            "word_count_caliber": appearance.word_count_caliber.map(|caliber| caliber.as_str()),
        }));
    }
    let settings = serde_json::json!({
        "format": "yanmo-settings",
        "version": SETTINGS_VERSION,
        "engine_version": version::engine_version(),
        "data_format_version": version::DATA_FORMAT_VERSION,
        "global": {
            "naming": global.naming.map(|naming| naming.as_str()),
            "chapter_numbering": global.chapter_numbering.as_str(),
            "word_count_caliber": global.word_count_caliber.map(|caliber| caliber.as_str()),
        },
        "works": settings_works,
    });
    let settings_bytes = serde_json::to_vec_pretty(&settings).unwrap_or_default();

    let mut extras = Vec::new();
    for (relative, bytes) in [(CATALOG_FILE, &catalog_bytes), (SETTINGS_FILE, &settings_bytes)] {
        let path = staging.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;
        extras.push(ExtraFile {
            name: relative.to_string(),
            bytes: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        });
    }
    Ok(extras)
}

/// 核对 extras：文件在、字节数对、SHA-256 对；目录索引与清单逐书对账。
pub(super) fn verify(package: &Path, extras: &[ExtraFile], works: &[WorkStamp]) -> Vec<String> {
    let mut problems = Vec::new();
    for extra in extras {
        let path = package.join(&extra.name);
        match std::fs::read(&path) {
            Err(error) => problems.push(format!("{} 读不到：{error}", extra.name)),
            Ok(bytes) => {
                if bytes.len() as u64 != extra.bytes {
                    problems.push(format!(
                        "{} 的大小对不上（清单 {}，实际 {}）",
                        extra.name,
                        extra.bytes,
                        bytes.len()
                    ));
                } else if sha256_hex(&bytes) != extra.sha256 {
                    problems.push(format!("{} 的内容与清单对不上", extra.name));
                }
            }
        }
    }

    if let Ok(bytes) = std::fs::read(package.join(CATALOG_FILE)) {
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Err(error) => problems.push(format!("{CATALOG_FILE} 不是合法 JSON：{error}")),
            Ok(catalog) => {
                let listed = catalog["works"].as_array().cloned().unwrap_or_default();
                if listed.len() != works.len() {
                    problems.push(format!(
                        "目录索引里的书数对不上：索引 {} 本，清单 {} 本",
                        listed.len(),
                        works.len()
                    ));
                }
                for work in works {
                    let Some(entry) =
                        listed.iter().find(|item| item["id"].as_i64() == Some(work.id))
                    else {
                        problems.push(format!("目录索引里缺《{}》", work.title));
                        continue;
                    };
                    if entry["chapters"].as_i64() != Some(work.chapters)
                        || entry["word_count"].as_i64() != Some(work.word_count)
                    {
                        problems.push(format!("目录索引里《{}》的章数/字数与清单不符", work.title));
                    }
                    if entry["fingerprint"].as_str() != Some(work.fingerprint.as_str()) {
                        problems.push(format!("目录索引里《{}》的成稿指纹与清单不符", work.title));
                    }
                }
            }
        }
    }

    if let Ok(bytes) = std::fs::read(package.join(SETTINGS_FILE)) {
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Err(error) => problems.push(format!("{SETTINGS_FILE} 不是合法 JSON：{error}")),
            Ok(settings) => {
                // 设置快照里**不许**出现本机标识（防回归：谁加了字段，这里当场红）
                let text = String::from_utf8_lossy(&bytes).to_lowercase();
                for forbidden in ["device_id", "session", "data_dir", "pointer"] {
                    if text.contains(forbidden) {
                        problems.push(format!("{SETTINGS_FILE} 里出现了本机标识：{forbidden}"));
                    }
                }
                let listed = settings["works"].as_array().cloned().unwrap_or_default();
                for work in works {
                    if !listed.iter().any(|item| item["id"].as_i64() == Some(work.id)) {
                        problems.push(format!("设置快照里缺《{}》", work.title));
                    }
                }
            }
        }
    }
    problems
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}
