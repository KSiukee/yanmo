//! 从成稿导入（**写**的那一半）：不依赖 SQLite 的最后一条回家路。
//!
//! 库彻底打不开的时候（文件坏了 / 盘上只剩一份导出），备份包里的
//! `成稿/<书名>/work.json` 仍然写着"目录长什么样、每章正文是什么"。
//! 成稿的**读**在 [`super::draft`]（纯函数，可脱离数据库单测）；这里只管把它写进库。
//!
//! # 三条规矩
//!
//! 1. **要么整本都进，要么一个字都不进**：结构与正文在**同一个事务**里写完；
//! 2. **一个字都不猜**：导进来的行没有真实创作时间，所以**不记账**（不计入每日码字），
//!    也**不逐章伪造**"每章各自诞生"的变更历史——留痕是研墨的头号差异化，
//!    伪造它等于把这份数据的可信度整块毁掉。留痕里只记"这一次导入"这一件事；
//! 3. **写完从库里读回来**：规模账与指纹都读库里的值，不是照着成稿念一遍
//!    （读回来才能跟备份清单对账，见 [`Store::imported_scale`]）。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::backup::{read_manifest, BackupManifest, DRAFT_DIR};
use super::Store;
use super::draft::{DraftScale, NodeDraft, WorkDraft};
use crate::error::Result;
use crate::model::{NodeKind, NamingStyle, WorkLanguage};

/// 导入的账：写完之后**从库里读回来**的数（不是照着成稿念一遍）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub work_id: i64,
    pub scale: DraftScale,
    /// 库里重渲染出来的分章文本指纹（与备份清单同一个口径，见 [`Store::draft_fingerprint`]）
    pub fingerprint: String,
}

impl Store {
    /// 把一份成稿写成一本书。**只新建**：既有作品一个字都不动。
    ///
    /// `source` 只进留痕（"这本书是从哪份成稿来的"），不参与任何逻辑。
    pub fn import_draft(
        &mut self,
        draft: &WorkDraft,
        language: WorkLanguage,
        source: &str,
    ) -> Result<ImportReport> {
        let tx = self.conn.transaction()?;
        let work_id = super::work::insert_work(&tx, draft.kind, &draft.title, language)?;
        for (slot, node) in draft.nodes.iter().enumerate() {
            write_node(&tx, work_id, None, node, slot as i64)?;
        }
        // 编号档跟着成稿落库：新书还没有偏好记录，所以这里写的就是"只这一项"的那份
        if let Some(naming) = draft.naming {
            super::appearance::write_appearance(
                &tx,
                Some(work_id),
                &super::Appearance {
                    naming: Some(naming.as_str().to_string()),
                    ..Default::default()
                },
            )?;
        }
        tx.commit()?;

        // 留痕：**记这一次导入这件事**（见文件头第 2 条）
        self.record(
            "works",
            work_id,
            "import",
            serde_json::json!({
                "source": source,
                "kind": draft.kind.as_str(),
                "language": language.as_str(),
                "naming": draft.naming.map(NamingStyle::as_str),
            }),
        )?;

        let scale = self.imported_scale(work_id)?;
        Ok(ImportReport { work_id, scale, fingerprint: self.draft_fingerprint(work_id)? })
    }

    /// 把一本书的规模**从库里读回来**（导入之后对账要用；与成稿算的那份口径一致）。
    pub fn imported_scale(&self, work_id: i64) -> Result<DraftScale> {
        let mut scale = DraftScale::default();
        for node in self.list_nodes(work_id)? {
            scale.nodes += 1;
            if node.kind == NodeKind::Chapter {
                scale.chapters += 1;
            }
            if node.has_body {
                scale.bodies += 1;
            }
            if node.title != node.title_rendered {
                scale.templated += 1;
            }
            scale.word_count += node.word_count;
            scale.char_count += node.char_count;
            scale.chars_no_punct += node.chars_no_punct;
        }
        Ok(scale)
    }
}

fn write_node(
    tx: &rusqlite::Connection,
    work_id: i64,
    parent: Option<i64>,
    node: &NodeDraft,
    slot: i64,
) -> Result<()> {
    let id = super::node_edit::insert_node(tx, work_id, parent, node.kind, &node.title, slot)?;
    if let Some(body) = &node.body {
        // 正文一个字不改地写进去：成稿里写着什么，书里就是什么（**不记账**，见文件头）
        super::content::put_body(tx, id, body, super::content::stats_of(body))?;
    }
    for (index, child) in node.children.iter().enumerate() {
        write_node(tx, work_id, Some(id), child, index as i64)?;
    }
    Ok(())
}

/// 在一个目录里找成稿文件（备份包目录、`成稿/` 目录、直接放着 work.json 的目录都认）。
///
/// 认的是**位置**，不是"能不能解析"：找到了却解析不了 → 那是坏成稿，照样当场报错，
/// 不许当没看见（这里只找文件，读与验在调用方）。顺序按路径排——每次跑都一样。
pub fn find_drafts(root: &Path) -> Vec<PathBuf> {
    let mut found: BTreeSet<PathBuf> = BTreeSet::new();
    let direct = root.join("work.json");
    if direct.is_file() {
        found.insert(direct);
    }
    // "成稿"这一层可能与 root 是同一个目录（作者直接把成稿文件夹本身指过来）
    let drafts_dir = if root.file_name().and_then(|name| name.to_str()) == Some(DRAFT_DIR) {
        root.to_path_buf()
    } else {
        root.join(DRAFT_DIR)
    };
    if let Ok(entries) = std::fs::read_dir(&drafts_dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // 我们自己写在包里的那些 JSON 不是成稿（清单 / 账本副本）
            let ours = name == super::backup::MANIFEST_FILE || name == super::backup::LEDGER_FILE;
            // 老布局（一份包里只有一本书）与手拷出来的那一份：成稿/<书名>.json
            if !ours && path.is_file() && path.extension().map(|ext| ext == "json").unwrap_or(false)
            {
                found.insert(path.clone());
            }
            // 现在一本书一个目录：成稿/<书名>/work.json
            let nested = path.join("work.json");
            if nested.is_file() {
                found.insert(nested);
            }
        }
    }
    found.into_iter().collect()
}

/// 从成稿文件往上找那份备份清单（不是备份包就返回 `None`——那就只导入、不对账）。
pub fn manifest_near(draft: &Path) -> Option<(PathBuf, BackupManifest)> {
    let mut at = draft.parent()?;
    for _ in 0..3 {
        if let Some(manifest) = read_manifest(at) {
            return Some((at.to_path_buf(), manifest));
        }
        at = at.parent()?;
    }
    None
}
