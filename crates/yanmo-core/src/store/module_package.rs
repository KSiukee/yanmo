//! 模块包校验：**安装前与宿主执行前共用同一套规则**。
//!
//! 入口 `.wasm`、大小上限、包内无原生可执行 / 符号链接、条目/深度上限、SHA-256 指纹。
//! 这里只做**静态检查**，不解析 WASM、不执行任何东西。
//!
//! ⚠️ 规则必须与 `yanmo-host` 的包校验保持一致：宿主是执行前的强制点，这里是安装前的
//! 早警告。两处都改才算改完。

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use yanmo_proto::manifest::ModuleManifest;
use yanmo_proto::{ManifestError, PROTOCOL_VERSION};

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

pub(super) struct OpenedPackage {
    pub(super) root: PathBuf,
    pub(super) manifest: ModuleManifest,
    pub(super) entry_bytes: u64,
    pub(super) fingerprint: String,
}

pub(super) fn open_package(dir: &Path) -> Result<OpenedPackage> {
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
    Ok(OpenedPackage {
        root,
        manifest,
        entry_bytes: meta.len(),
        fingerprint,
    })
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

pub(super) fn package(detail: impl Into<String>) -> Error {
    Error::invalid_with(codes::MODULE_PACKAGE_INVALID, [("detail", detail.into())])
}
