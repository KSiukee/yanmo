//! 更新纪律的**回滚**那一半：升级时把上一版留档，出事能退回去。
//!
//! # 留档放在哪
//!
//! `<数据目录>/module-archive/<模块 id>/<版本>/`——**不在**模块平台的扫描根
//! （`<数据目录>/modules/`）里面：平台启动时会把扫描根下每个带 `manifest.json`
//! 的目录当成一个模块拉起来，留档要是混进去，等于凭空多出几个同 id 的模块。
//!
//! 每个留档目录里两件东西：
//! - `package/`：那一版的完整包（复制件，不是软链）；
//! - `record.json`：那一版的清单、授予、指纹与留档时刻——**回滚连权限一起退**。
//!
//! # 回滚是一次性的
//!
//! 回滚成功后留档就被收掉：它本来就是"上一版"的位置，退回去之后没有更早的可退。
//! 想再升上去，重新导入那一版（或更新的一版）即可——**不静默、不自动**，这正是这条纪律。
//!
//! # 留档失败不阻断升级
//!
//! 磁盘满 / 权限不足时留档会失败：这时升级照常做（回执里 `archived: false`），
//! 界面如实说"这一版没能留档，回滚不了"。**拿"回滚不了"去拦一次正常升级**，
//! 是把可修的问题当成了致命错误。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::manifest::ModuleManifest;

use super::certification::CertificationSet;
use super::module_import::{copy_package_swapping, copy_tree, ensure_protocol};
use super::module_package::open_package;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 数据目录里的留档根（壳拼给核心；**不在模块扫描根下面**）。
pub const ARCHIVE_DIR: &str = "module-archive";

/// 一份留档：那一版的包 + 账。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveRecord {
    pub module_id: String,
    pub version: String,
    pub fingerprint: String,
    pub manifest: ModuleManifest,
    pub granted: ModuleCapabilities,
    pub archived_at: i64,
}

impl ArchiveRecord {
    /// 留档里的包在哪儿。
    pub fn package_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.module_id).join(&self.version).join("package")
    }
}

/// 回滚的回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RollbackReceipt {
    pub module_id: String,
    pub from_version: String,
    pub to_version: String,
}

/// 把**当前**这一版留档（升级之前调；同一版本重复留档会覆盖）。
///
/// 返回留档目录（失败时由调用方决定：升级照常、只是回执里说没留成）。
pub fn archive_version(
    archive_root: &Path,
    module_id: &str,
    version: &str,
    fingerprint: &str,
    manifest: &ModuleManifest,
    granted: &ModuleCapabilities,
    package_dir: &Path,
) -> std::io::Result<()> {
    let dir = archive_root.join(module_id).join(version);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(dir.join("package"))?;
    let record = ArchiveRecord {
        module_id: module_id.to_string(),
        version: version.to_string(),
        fingerprint: fingerprint.to_string(),
        manifest: manifest.clone(),
        granted: granted.clone(),
        archived_at: now_millis(),
    };
    let text = serde_json::to_vec_pretty(&record)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    fs::write(dir.join("record.json"), text)?;
    // 包本体：复制（不是软链），留着的是"当时一模一样"的那一份
    if let Err(error) = copy_archive_tree(package_dir, &dir.join("package")) {
        let _ = fs::remove_dir_all(&dir);
        return Err(std::io::Error::other(error));
    }
    prune_older_versions(&dir, version);
    Ok(())
}

fn copy_archive_tree(source: &Path, dest: &Path) -> Result<()> {
    // 复用导入那套复制（含软链拒绝），但错误换成 io 口径给调用方
    copy_tree(source, dest)
}

/// 同一模块只留最新一份留档（任务口径是"保留上一版本"）。
fn prune_older_versions(version_dir: &Path, keep: &str) {
    let Some(module_dir) = version_dir.parent() else { return };
    let Ok(entries) = fs::read_dir(module_dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy() == keep {
            continue;
        }
        let _ = fs::remove_dir_all(entry.path());
    }
}

/// 读这个模块的留档（没有就是 `None`；留档坏了也当没有，回滚前还会再核对一次）。
pub fn read_archive(archive_root: &Path, module_id: &str) -> Option<ArchiveRecord> {
    let module_dir = archive_root.join(module_id);
    let entries = fs::read_dir(module_dir).ok()?;
    let mut newest: Option<ArchiveRecord> = None;
    for entry in entries.flatten() {
        let path = entry.path().join("record.json");
        let Ok(text) = fs::read(&path) else { continue };
        let Ok(record) = serde_json::from_slice::<ArchiveRecord>(&text) else { continue };
        if record.module_id != module_id {
            continue;
        }
        if newest.as_ref().map(|current| record.archived_at > current.archived_at).unwrap_or(true) {
            newest = Some(record);
        }
    }
    newest
}

impl Store {
    /// **回滚到留档那一版**：包换回去 + 账换代（连授予一起退），数据不动。
    ///
    /// 三条纪律：
    /// 1. 只在有留档时可用（没有 → `module.rollback_unavailable`）；
    /// 2. 留档里的包要**重新算指纹**：跟留档记的对不上就拒（留档也可能被动过）；
    /// 3. 换代失败就把新包换回去（与升级同一条：不留"包新账旧"的半套）。
    ///
    /// 成功后留档**收掉**（它是"上一版"的位置，退回去之后没有更早的可退）。
    pub fn rollback_module_managed(
        &mut self,
        archive_root: &Path,
        dest_dir: &Path,
        module_id: &str,
        certs: &CertificationSet,
    ) -> Result<RollbackReceipt> {
        let record = read_archive(archive_root, module_id).ok_or_else(|| {
            Error::invalid_with(codes::MODULE_ROLLBACK_UNAVAILABLE, [("module_id", module_id.to_string())])
        })?;
        let package = record.package_dir(archive_root);
        let opened = open_package(&package)?;
        ensure_protocol(&opened.manifest)?;
        if opened.manifest.id != module_id || opened.fingerprint != record.fingerprint {
            return Err(Error::invalid(codes::MODULE_PACKAGE_TAMPERED));
        }
        let current = self.module_record(module_id)?;
        let backup = copy_package_swapping(&opened, dest_dir)?;
        let certification =
            certs.lookup(&opened.manifest.id, &opened.manifest.version, &opened.fingerprint);
        let replaced = self.replace_module(
            &record.manifest,
            &record.granted,
            &record.fingerprint,
            "rollback",
            certification,
        );
        match replaced {
            Ok(()) => {
                if let Some(backup) = backup {
                    let _ = fs::remove_dir_all(backup);
                }
                // 留档收掉：回滚是一次性的
                let _ = fs::remove_dir_all(archive_root.join(module_id));
                Ok(RollbackReceipt {
                    module_id: module_id.to_string(),
                    from_version: current.manifest.version,
                    to_version: record.version,
                })
            }
            Err(error) => {
                if let Some(backup) = backup {
                    let _ = fs::remove_dir_all(dest_dir);
                    let _ = fs::rename(backup, dest_dir);
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> ModuleManifest {
        let text = serde_json::json!({
            "id": "app.example.archived",
            "default_language": "zh-Hans",
            "name": { "zh-Hans": "留档测试" },
            "description": { "zh-Hans": "只用来测留档。" },
            "author": { "name": "Example Studio" },
            "version": version,
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": ["works"] }
        });
        ModuleManifest::from_json(&text.to_string()).unwrap()
    }

    fn write_package(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("manifest.json"), "{}").unwrap();
        fs::write(dir.join("module.wasm"), b"wasm").unwrap();
    }

    #[test]
    fn archiving_then_reading_returns_the_newest_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let package = dir.path().join("pkg");
        write_package(&package);
        archive_version(root, "app.example.archived", "1.0.0", "abc", &manifest("1.0.0"),
            &ModuleCapabilities::none(), &package).unwrap();
        // 第二版留档会顶掉第一版（只留"上一版"）
        archive_version(root, "app.example.archived", "1.1.0", "def", &manifest("1.1.0"),
            &ModuleCapabilities::none(), &package).unwrap();
        let record = read_archive(root, "app.example.archived").unwrap();
        assert_eq!(record.version, "1.1.0");
        assert!(record.package_dir(root).join("module.wasm").is_file());
        assert!(
            !root.join("app.example.archived").join("1.0.0").exists(),
            "同一模块只留最新一份"
        );
    }

    #[test]
    fn a_missing_archive_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_archive(dir.path(), "app.example.none").is_none());
    }
}
