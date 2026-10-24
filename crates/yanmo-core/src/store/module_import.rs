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
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::localized::localized_text;
use yanmo_proto::manifest::ModuleManifest;
use yanmo_proto::{ManifestError, PROTOCOL_VERSION};

use super::Store;
use crate::error::{codes, Error, Result};

const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PACKAGE_ENTRIES: usize = 2_000;
const MAX_PACKAGE_DEPTH: usize = 8;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// 与宿主包校验同一条清单：包内一律不许出现的原生可执行 / 脚本后缀。
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "drv", "msi", "com", "scr", "bat", "cmd", "ps1", "vbs", "js", "jse",
    "wsf", "wsh", "cpl", "ocx",
];

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
}

/// 检查一个模块目录，返回预览；**不安装**。
pub fn inspect_module_package(dir: impl AsRef<Path>) -> Result<ImportPreview> {
    let opened = open_package(dir.as_ref())?;
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
        let opened = open_package(dir.as_ref())?;
        ensure_protocol(&opened.manifest)?;
        if !expected_fingerprint.is_empty() && expected_fingerprint != opened.fingerprint {
            return Err(Error::invalid(codes::MODULE_IMPORT_CHANGED));
        }
        self.insert_module(
            &opened.manifest,
            granted,
            &opened.fingerprint,
            source,
            "import",
        )
    }
}

fn ensure_protocol(manifest: &ModuleManifest) -> Result<()> {
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

struct OpenedPackage {
    manifest: ModuleManifest,
    entry_bytes: u64,
    fingerprint: String,
}

fn open_package(dir: &Path) -> Result<OpenedPackage> {
    let root = fs::canonicalize(dir)
        .map_err(|error| package(format!("not a directory: {error}")))?;
    if !root.is_dir() {
        return Err(package("not a directory"));
    }
    let manifest_path = root.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| package(format!("manifest.json: {error}")))?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(package("manifest.json is too large"));
    }
    let manifest_text = std::str::from_utf8(&manifest_bytes)
        .map_err(|error| package(format!("manifest.json: {error}")))?;
    let raw: serde_json::Value = serde_json::from_str(manifest_text)
        .map_err(|error| package(format!("manifest invalid: {error}")))?;
    let declared_protocol = raw.get("protocol").and_then(|value| value.as_u64()).unwrap_or(0) as u32;
    let manifest = match ModuleManifest::from_json(manifest_text) {
        Ok(manifest) => manifest,
        Err(error) if error.code == ManifestError::PROTOCOL => {
            return Err(Error::invalid_with(
                codes::MODULE_PROTOCOL_INCOMPATIBLE,
                [
                    ("actual", declared_protocol.to_string()),
                    ("expected", PROTOCOL_VERSION.to_string()),
                ],
            ));
        }
        Err(error) => return Err(package(format!("manifest invalid: {error}"))),
    };

    let entry = root.join(&manifest.entry);
    let meta = fs::symlink_metadata(&entry)
        .map_err(|error| package(format!("entry_missing: {}: {error}", manifest.entry)))?;
    if meta.file_type().is_symlink() {
        return Err(package("symlink: entry"));
    }
    if !meta.is_file() {
        return Err(package("entry_missing: entry is not a file"));
    }
    if !entry
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wasm"))
    {
        return Err(package("entry_not_wasm: entry must be .wasm"));
    }
    if meta.len() > MAX_ENTRY_BYTES {
        return Err(package(format!("entry_too_large: {} bytes", meta.len())));
    }
    let canonical_entry =
        fs::canonicalize(&entry).map_err(|error| package(format!("entry_missing: {error}")))?;
    if !canonical_entry.starts_with(&root) {
        return Err(package("entry escapes the module directory"));
    }

    let mut total = 0u64;
    let mut count = 0usize;
    scan(&root, 0, &mut total, &mut count)?;

    let bytes = fs::read(&canonical_entry)
        .map_err(|error| package(format!("entry read: {error}")))?;
    let digest = Sha256::digest(&bytes);
    let fingerprint = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(OpenedPackage { manifest, entry_bytes: meta.len(), fingerprint })
}

fn scan(dir: &Path, depth: usize, total: &mut u64, count: &mut usize) -> Result<()> {
    if depth > MAX_PACKAGE_DEPTH {
        return Err(package("directory nesting is too deep"));
    }
    let entries = fs::read_dir(dir)
        .map_err(|error| package(format!("read_dir {}: {error}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| package(error.to_string()))?;
        *count += 1;
        if *count > MAX_PACKAGE_ENTRIES {
            return Err(package("too many entries"));
        }
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)
            .map_err(|error| package(error.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(package(format!("symlink: {}", path.display())));
        }
        if meta.is_dir() {
            scan(&path, depth + 1, total, count)?;
            continue;
        }
        if !meta.is_file() {
            return Err(package("unsupported file type"));
        }
        *total = total.saturating_add(meta.len());
        if *total > MAX_PACKAGE_BYTES {
            return Err(package("package is too large"));
        }
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            if EXECUTABLE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()) {
                return Err(package(format!(
                    "executable: {} is not allowed in a module package",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn package(detail: impl Into<String>) -> Error {
    Error::invalid_with(codes::MODULE_PACKAGE_INVALID, [("detail", detail.into())])
}
