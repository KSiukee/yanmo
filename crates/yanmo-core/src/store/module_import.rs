//! 模块导入闸：**结构完整 + 协议兼容 + 载荷指纹**，外加把声明原样交给界面。
//!
//! 这里只做**事实核对**：包在不在、入口是不是 `.wasm`、有没有原生可执行 / 符号链接、
//! 协议版本对不对、载荷 SHA-256 是多少。**不判断"正版"**（授权由模块自己离线握手），
//! 也不执行任何东西。
//!
//! ⚠️ 这些包校验规则必须与 `yanmo-host` 的包校验保持一致：宿主是**执行前**的强制点，
//! 这里是**安装前**的早警告。两处都改才算改完。
//!
//! 指纹在导入时算一次并落注册表；认证状态将来绑定它（认证那一刀）。`source` 只存短标签
//! （如 `folder`），**不存完整本机路径**。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::localized::localized_text;
use yanmo_proto::manifest::ModuleManifest;
use yanmo_proto::PROTOCOL_VERSION;

use super::certification::CertificationSet;
use super::module_package::{open_package, package, OpenedPackage};
use super::Store;
use crate::error::{codes, Error, Result};

/// 导入前的预览：**只给事实**，界面据此逐项征得同意。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportPreview {
    pub module_id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub protocol: u32,
    pub expected_protocol: u32,
    pub compatible: bool,
    pub fingerprint: String,
    pub declared: ModuleCapabilities,
    pub entry_bytes: u64,
    /// 认证是**核心验签后的结果**，绝不是模块自述。
    pub certified: bool,
    pub cert_edition: u32,
    pub certified_at: i64,
    /// 库里已经装着同一个 id（升级 / 同版 / 降级三种情形都从这里看）。
    pub installed: bool,
    /// 现在装着的版本（没装就是空串）。
    pub installed_version: String,
    /// 现在装着的那个版本的说明（界面拿它跟新说明对比，有变化就说"变更说明"）。
    pub installed_description: String,
    /// 新版本**严格高于**装着的版本 → 可以升级。
    pub upgrade: bool,
    /// 新版本不比装着的版本新（≤）——按纪律拒，不许降级/重装覆盖。
    pub downgrade: bool,
    /// 现在授予的能力（没装就是全不给）——界面拿它预勾选。
    pub granted: ModuleCapabilities,
    /// 这次会**新增**的能力码（`read.body` / `own_data` / `network` / `ask_questions` /
    /// `cross_read.<id>`）——空表示能力没扩大。
    pub added: Vec<String>,
}

/// 检查一个模块目录，返回预览；**不安装**。
pub fn inspect_module_package(dir: impl AsRef<Path>) -> Result<ImportPreview> {
    inspect_module_package_with_certs(dir, &CertificationSet::built_in())
}

/// 同 [`inspect_module_package`]，但用指定的已验签清单（测试 / 将来的签发工具用）。
pub fn inspect_module_package_with_certs(
    dir: impl AsRef<Path>,
    certs: &CertificationSet,
) -> Result<ImportPreview> {
    let opened = open_package(dir.as_ref())?;
    let certification =
        certs.lookup(&opened.manifest.id, &opened.manifest.version, &opened.fingerprint);
    Ok(ImportPreview {
        module_id: opened.manifest.id.clone(),
        name: localized_text(
            &opened.manifest.name,
            &opened.manifest.default_language,
            "zh-Hans",
        )
        .to_string(),
        description: localized_text(
            &opened.manifest.description,
            &opened.manifest.default_language,
            "zh-Hans",
        )
        .to_string(),
        version: opened.manifest.version.clone(),
        author: opened.manifest.author.name.clone(),
        protocol: opened.manifest.protocol,
        expected_protocol: PROTOCOL_VERSION,
        compatible: opened.manifest.protocol == PROTOCOL_VERSION,
        fingerprint: opened.fingerprint,
        declared: opened.manifest.capabilities.clone(),
        entry_bytes: opened.entry_bytes,
        certified: certification.is_some(),
        cert_edition: certification.map(|cert| cert.edition).unwrap_or(0),
        certified_at: certification.map(|cert| cert.certified_at).unwrap_or(0),
        installed: false,
        installed_version: String::new(),
        installed_description: String::new(),
        upgrade: false,
        downgrade: false,
        granted: ModuleCapabilities::none(),
        added: Vec::new(),
    })
}

impl Store {
    /// 导入并安装：**重新校验 + 重新算指纹**，指纹与预览时不一致就拒。
    ///
    /// 重复安装同一个 id 仍由注册表拒（升级走更新纪律）。
    pub fn import_module(
        &mut self,
        dir: impl AsRef<Path>,
        granted: &ModuleCapabilities,
        expected_fingerprint: &str,
        source: &str,
    ) -> Result<()> {
        self.import_module_with_certs(
            dir,
            granted,
            expected_fingerprint,
            source,
            &CertificationSet::built_in(),
        )
    }

    /// 同 [`Store::import_module`]，但用指定的已验签清单。
    pub fn import_module_with_certs(
        &mut self,
        dir: impl AsRef<Path>,
        granted: &ModuleCapabilities,
        expected_fingerprint: &str,
        source: &str,
        certs: &CertificationSet,
    ) -> Result<()> {
        let opened = open_package(dir.as_ref())?;
        ensure_protocol(&opened.manifest)?;
        if !expected_fingerprint.is_empty() && expected_fingerprint != opened.fingerprint {
            return Err(Error::invalid(codes::MODULE_IMPORT_CHANGED));
        }
        let certification =
            certs.lookup(&opened.manifest.id, &opened.manifest.version, &opened.fingerprint);
        self.insert_module(
            &opened.manifest,
            granted,
            &opened.fingerprint,
            source,
            "import",
            certification,
        )
    }

    /// 导入并把包**复制进受管目录**（原子换入 + 复制后复核指纹），再落注册表。
    ///
    /// `dest_dir` 由壳给出（如 `<data>/modules/<id>`）；核心不猜数据目录。
    pub fn import_module_managed(
        &mut self,
        source_dir: impl AsRef<Path>,
        dest_dir: impl AsRef<Path>,
        granted: &ModuleCapabilities,
        expected_fingerprint: &str,
        source_label: &str,
        certs: &CertificationSet,
    ) -> Result<()> {
        let opened = open_package(source_dir.as_ref())?;
        ensure_protocol(&opened.manifest)?;
        if !expected_fingerprint.is_empty() && expected_fingerprint != opened.fingerprint {
            return Err(Error::invalid(codes::MODULE_IMPORT_CHANGED));
        }
        match self.module_record(&opened.manifest.id) {
            Ok(_) => {
                return Err(Error::invalid_with(
                    codes::MODULE_ALREADY_INSTALLED,
                    [("module_id", opened.manifest.id.clone())],
                ))
            }
            Err(error) if error.code() == codes::MODULE_NOT_FOUND => {}
            Err(error) => return Err(error),
        }

        let dest = dest_dir.as_ref();
        copy_package(&opened, dest)?;
        let certification =
            certs.lookup(&opened.manifest.id, &opened.manifest.version, &opened.fingerprint);
        let result = self.insert_module(
            &opened.manifest,
            granted,
            &opened.fingerprint,
            source_label,
            "import",
            certification,
        );
        if result.is_err() {
            let _ = fs::remove_dir_all(dest);
        }
        result
    }

    /// 启动前核对：受管目录里的包必须仍是导入时那一份（id + 载荷哈希）。
    pub fn verify_module_package(
        &self,
        module_id: &str,
        package_dir: impl AsRef<Path>,
    ) -> Result<()> {
        let card = self.module_card(module_id)?;
        if card.fingerprint.is_empty() {
            return Err(Error::invalid(codes::MODULE_PACKAGE_TAMPERED));
        }
        let preview = inspect_module_package(package_dir)?;
        if preview.module_id != card.id || preview.fingerprint != card.fingerprint {
            return Err(Error::invalid(codes::MODULE_PACKAGE_TAMPERED));
        }
        Ok(())
    }
}

fn copy_package(opened: &OpenedPackage, dest: &Path) -> Result<()> {
    if let Some(backup) = copy_package_swapping(opened, dest)? {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

/// 换包但**把旧包留在旁边**（`.旧名.old-<pid>-<时间>-<序号>`）；调用方负责删或换回来。
pub(super) fn copy_package_swapping(opened: &OpenedPackage, dest: &Path) -> Result<Option<PathBuf>> {
    let source = &opened.root;
    if dest == source || dest.starts_with(source) || source.starts_with(dest) {
        return Err(package("managed destination overlaps the source package"));
    }
    let parent = dest
        .parent()
        .ok_or_else(|| package("managed destination has no parent"))?;
    fs::create_dir_all(parent).map_err(io_package)?;

    let temp = sibling(dest, "tmp");
    if temp.exists() {
        fs::remove_dir_all(&temp).map_err(io_package)?;
    }
    copy_tree(source, &temp)?;

    let copied = temp.join(&opened.manifest.entry);
    if sha256_file(&copied)? != opened.fingerprint {
        let _ = fs::remove_dir_all(&temp);
        return Err(package("copied payload hash does not match the verified payload"));
    }

    let backup = sibling(dest, "old");
    let had_old = dest.exists();
    if had_old {
        fs::rename(dest, &backup).map_err(io_package)?;
    }
    if let Err(error) = fs::rename(&temp, dest) {
        if had_old {
            let _ = fs::rename(&backup, dest);
        }
        let _ = fs::remove_dir_all(&temp);
        return Err(io_package(error));
    }
    if had_old {
        Ok(Some(backup))
    } else {
        Ok(None)
    }
}

pub(super) fn copy_tree(source: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).map_err(io_package)?;
    for entry in fs::read_dir(source).map_err(io_package)? {
        let entry = entry.map_err(io_package)?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(io_package)?;
        if meta.file_type().is_symlink() {
            return Err(package(format!("symlink: {}", path.display())));
        }
        let target = dest.join(entry.file_name());
        if meta.is_dir() {
            copy_tree(&path, &target)?;
        } else if meta.is_file() {
            fs::copy(&path, &target).map_err(io_package)?;
        } else {
            return Err(package("unsupported file type"));
        }
    }
    Ok(())
}

fn sibling(dest: &Path, kind: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("module");
    dest.with_file_name(format!(
        ".{name}.{kind}-{}-{}-{seq}",
        std::process::id(),
        crate::time::now_millis()
    ))
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(io_package)?;
    let digest = Sha256::digest(&bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn io_package(error: std::io::Error) -> Error {
    package(error.to_string())
}

pub(super) fn ensure_protocol(manifest: &ModuleManifest) -> Result<()> {
    if manifest.protocol == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(Error::invalid_with(
            codes::MODULE_PROTOCOL_INCOMPATIBLE,
            [
                ("actual", manifest.protocol.to_string()),
                ("expected", PROTOCOL_VERSION.to_string()),
            ],
        ))
    }
}
