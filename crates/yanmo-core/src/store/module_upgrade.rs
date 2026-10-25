//! 模块升级：**同 id 换新版本**。
//!
//! 三件事：预览里那三件事实（升/降/新增能力）、包与账一起换（失败把旧包换回来）、
//! 以及"授予不得越过新声明"的同一条纪律。检查与安装的基础件仍在
//! [`super::module_import`]，注册表的列与留痕在 [`super::module_registry`]。

use std::fs;
use std::path::Path;

use rusqlite::params;
use serde::Serialize;
use serde_json::json;
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::localized::localized_text;
use yanmo_proto::manifest::ModuleManifest;
use yanmo_proto::ManifestError;
use yanmo_proto::version::parse_semver;

use super::certification::CertificationSet;
use super::module_import::{
    copy_package_swapping, ensure_protocol, inspect_module_package, ImportPreview,
};
use super::module_package::open_package;
use super::module_registry::{encode_grant, map_grant_error, map_manifest_error, not_found};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::first_ungrantable;
use crate::time::now_millis;

/// 一次升级的回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpgradeReceipt {
    pub module_id: String,
    pub from_version: String,
    pub to_version: String,
    /// 这次升级**新增**的能力码（空 = 能力没扩大，界面不用再确认）。
    pub added: Vec<String>,
    /// 上一版留成档了没有（留成才能回滚；失败不拦升级，界面如实说）。
    pub archived: bool,
}

impl Store {
    /// 导入前的预览（已在店里装了同一个 id 时，带上"升级 / 降级 / 新增能力"这三件事实）。
    ///
    /// 与 [`inspect_module_package`] 的区别只有一个：那份不碰库，这份会看一眼注册表。
    pub fn inspect_import(&self, dir: impl AsRef<Path>) -> Result<ImportPreview> {
        let mut preview = inspect_module_package(dir)?;
        let Ok(record) = self.module_record(&preview.module_id) else {
            return Ok(preview);
        };
        preview.installed = true;
        preview.installed_version = record.manifest.version.clone();
        preview.installed_description = localized_text(
            &record.manifest.description,
            &record.manifest.default_language,
            "zh-Hans",
        )
        .to_string();
        let order = compare_versions(&record.manifest.version, &preview.version);
        preview.upgrade = order > 0;
        preview.downgrade = order <= 0;
        preview.added = capability_additions(&preview.declared, &record.granted);
        preview.granted = record.granted;
        Ok(preview)
    }

    /// **升级**已装的模块：包原子换入 + 注册表换代，偏好与数据都不动。
    ///
    /// 三条纪律：
    /// 1. 只许升到**更高**的版本（同版 / 降级都拒——要回滚先卸载再装旧版）；
    /// 2. 授予照旧由调用方给（界面拿新声明重新征得同意，**新增的能力默认不给**）；
    /// 3. 注册表换代失败就把旧包换回来（不会留下"包新了、账还是旧"的半套）。
    #[allow(clippy::too_many_arguments)]
    pub fn upgrade_module_managed(
        &mut self,
        source_dir: impl AsRef<Path>,
        dest_dir: impl AsRef<Path>,
        archive_root: impl AsRef<Path>,
        granted: &ModuleCapabilities,
        expected_fingerprint: &str,
        source_label: &str,
        certs: &CertificationSet,
    ) -> Result<UpgradeReceipt> {
        let opened = open_package(source_dir.as_ref())?;
        ensure_protocol(&opened.manifest)?;
        if !expected_fingerprint.is_empty() && expected_fingerprint != opened.fingerprint {
            return Err(Error::invalid(codes::MODULE_IMPORT_CHANGED));
        }
        let current = self.module_record(&opened.manifest.id)?;
        if compare_versions(&current.manifest.version, &opened.manifest.version) <= 0 {
            return Err(Error::invalid_with(
                codes::MODULE_UPGRADE_NOT_NEWER,
                [
                    ("module_id", opened.manifest.id.clone()),
                    ("current", current.manifest.version.clone()),
                    ("found", opened.manifest.version.clone()),
                ],
            ));
        }
        let added = capability_additions(&opened.manifest.capabilities, &current.granted);
        let dest = dest_dir.as_ref();
        // 留档：把**当前这一版**（包 + 账）留一份，出事能退回去。
        // 留不成不拦升级——回执里如实说"这一版没能留档"。
        let archived = self
            .module_card(&opened.manifest.id)
            .map(|card| card.fingerprint)
            .ok()
            .map(|fingerprint| {
                super::module_archive::archive_version(
                    archive_root.as_ref(),
                    &current.manifest.id,
                    &current.manifest.version,
                    &fingerprint,
                    &current.manifest,
                    &current.granted,
                    dest,
                )
                .is_ok()
            })
            .unwrap_or(false);
        let backup = copy_package_swapping(&opened, dest)?;
        let certification =
            certs.lookup(&opened.manifest.id, &opened.manifest.version, &opened.fingerprint);
        let replaced = self.replace_module(
            &opened.manifest,
            granted,
            &opened.fingerprint,
            source_label,
            certification,
        );
        match replaced {
            Ok(()) => {
                if let Some(backup) = backup {
                    let _ = fs::remove_dir_all(backup);
                }
                Ok(UpgradeReceipt {
                    module_id: opened.manifest.id,
                    from_version: current.manifest.version,
                    to_version: opened.manifest.version,
                    added,
                    archived,
                })
            }
            Err(error) => {
                // 包已经换过去了：把旧包换回来，别留"包新账旧"的半套
                if let Some(backup) = backup {
                    let _ = fs::remove_dir_all(dest);
                    let _ = fs::rename(backup, dest);
                }
                Err(error)
            }
        }
    }
}

/// 版本比较：`1` = 新的更高，`0` = 一样，`-1` = 新的更低。
///
/// 版本号认不出来就当"不新"（拒）：宁可让作者看到"装不了"，也不拿字符串比较去猜高低。
fn compare_versions(current: &str, found: &str) -> i32 {
    match (parse_semver(current), parse_semver(found)) {
        (Some(current), Some(found)) => match found.cmp(&current) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Less => -1,
        },
        _ => 0,
    }
}

/// 新声明里**多出来**的能力（已授予的集合之外的那些），按稳定能力码给出。
pub(crate) fn capability_additions(
    declared: &ModuleCapabilities,
    granted: &ModuleCapabilities,
) -> Vec<String> {
    let mut added = Vec::new();
    for area in &declared.read {
        if !granted.read.contains(area) {
            added.push(format!("read.{}", area.as_str()));
        }
    }
    if declared.own_data && !granted.own_data {
        added.push("own_data".to_string());
    }
    if declared.network && !granted.network {
        added.push("network".to_string());
    }
    if declared.ask_questions && !granted.ask_questions {
        added.push("ask_questions".to_string());
    }
    for id in &declared.cross_read {
        if !granted.cross_read.contains(id) {
            added.push(format!("cross_read.{id}"));
        }
    }
    added
}

impl Store {
    /// **升级**：换清单 / 授予 / 指纹 / 认证状态，**保留**激活与偏好、安装时间不动。
    ///
    /// 只由 [`super::Store::upgrade_module_managed`] 调用（它负责先把包原子换好，
    /// 失败时把旧包换回来）。校验与 `insert_module` 同一套：清单合规、授予合规、
    /// 授予不得越过声明。
    pub(crate) fn replace_module(
        &mut self,
        manifest: &ModuleManifest,
        granted: &ModuleCapabilities,
        payload_sha256: &str,
        source: &str,
        certification: Option<&super::certification::Certification>,
    ) -> Result<()> {
        manifest.validate().map_err(map_manifest_error)?;
        granted.validate(&manifest.id).map_err(map_grant_error)?;
        if let Some(capability) = first_ungrantable(&manifest.capabilities, granted) {
            return Err(Error::invalid_with(
                codes::MODULE_GRANT_NOT_DECLARED,
                [("capability", capability)],
            ));
        }
        let manifest_text = serde_json::to_string(manifest).map_err(|_| {
            Error::invalid_with(
                codes::MODULE_MANIFEST_INVALID,
                [("code", ManifestError::JSON.to_string()), ("field", "manifest".to_string())],
            )
        })?;
        let granted_text = encode_grant(granted);
        let now = now_millis();
        let certified = i64::from(certification.is_some());
        let cert_edition = certification.map(|cert| cert.edition).unwrap_or(0) as i64;
        let certified_at = certification.map(|cert| cert.certified_at).unwrap_or(0);
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE modules
                SET manifest = ?2, version = ?3, protocol = ?4, certified = ?5,
                    cert_edition = ?6, certified_at = ?7, granted = ?8,
                    payload_sha256 = ?9, source = ?10, updated_at = ?11
              WHERE id = ?1",
            params![
                manifest.id,
                manifest_text,
                manifest.version,
                manifest.protocol,
                certified,
                cert_edition,
                certified_at,
                granted_text,
                payload_sha256,
                source,
                now
            ],
        )?;
        if changed == 0 {
            return Err(not_found(&manifest.id));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            "upgrade",
            json!({
                "module_id": manifest.id,
                "version": manifest.version,
                "protocol": manifest.protocol,
                "granted": granted,
                "payload_sha256": payload_sha256,
                "source": source,
                "certified": certification.is_some(),
            }),
        )?;
        tx.commit()?;
        Ok(())
    }
}
