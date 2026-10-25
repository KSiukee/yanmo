//! 模块包：**一个目录 = 一份清单 + 一个 `.wasm` 载荷 + 数据文件**。
//!
//! 这里只做**静态校验**，不执行任何东西：
//! - 清单必须过协议层校验（未知字段、坏 id、坏入口都拒）；
//! - 入口必须是目录内的 `.wasm` 文件，不是符号链接，不超过 32 MiB；
//! - 整包不超过 128 MiB / 2000 个条目；
//! - **包内不许出现原生可执行文件**（`.exe` / `.dll` / 脚本等）——不是"我们选择不跑"，
//!   而是**根本不允许带**；沙箱载荷在插件系统之外也跑不起来；
//! - 包内不许有符号链接（逃逸面直接归零）。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use yanmo_proto::ModuleManifest;

use crate::error::{HostError, HostResult};

/// 入口 `.wasm` 的大小上限。
pub const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
/// 整个模块目录的大小上限。
pub const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
/// 模块目录里最多多少个条目（含子目录与文件）。
pub const MAX_PACKAGE_ENTRIES: usize = 2_000;
/// 目录递归的最大深度。
pub const MAX_PACKAGE_DEPTH: usize = 8;
/// 清单文件的大小上限（它是一份声明，不该是正文）。
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// 包内一律不许出现的原生可执行 / 脚本后缀（**不是黑名单式防护，是"包格式"本身**）。
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "sys", "drv", "msi", "com", "scr", "bat", "cmd", "ps1", "vbs", "js", "jse",
    "wsf", "wsh", "cpl", "ocx",
];

/// 通过静态校验的模块包。
#[derive(Debug, Clone)]
pub struct ModulePackage {
    dir: PathBuf,
    manifest: ModuleManifest,
    entry: PathBuf,
    entry_bytes: u64,
}

impl ModulePackage {
    /// 打开并校验一个模块目录；**不执行载荷**。
    pub fn open(dir: impl AsRef<Path>) -> HostResult<Self> {
        let dir = dir.as_ref();
        let canonical_dir = fs::canonicalize(dir)
            .map_err(|error| HostError::new(HostError::PACKAGE, format!("not a directory: {error}")))?;
        if !canonical_dir.is_dir() {
            return Err(HostError::new(HostError::PACKAGE, "not a directory"));
        }

        let manifest_path = canonical_dir.join("manifest.json");
        let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
            HostError::new(HostError::PACKAGE, format!("manifest.json: {error}"))
        })?;
        if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(HostError::new(HostError::MANIFEST, "manifest.json is too large"));
        }
        let manifest_text = std::str::from_utf8(&manifest_bytes)
            .map_err(|error| HostError::new(HostError::MANIFEST, error.to_string()))?;
        let manifest = ModuleManifest::from_json(manifest_text)
            .map_err(|error| HostError::new(HostError::MANIFEST, error.to_string()))?;

        let entry = canonical_dir.join(&manifest.entry);
        let entry_meta = fs::symlink_metadata(&entry).map_err(|error| {
            HostError::new(HostError::ENTRY_MISSING, format!("{}: {error}", manifest.entry))
        })?;
        if entry_meta.file_type().is_symlink() {
            return Err(HostError::new(HostError::SYMLINK, "entry must not be a symlink"));
        }
        if !entry_meta.is_file() {
            return Err(HostError::new(HostError::ENTRY_MISSING, "entry is not a file"));
        }
        if !entry.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("wasm")) {
            return Err(HostError::new(HostError::ENTRY_NOT_WASM, "entry must be .wasm"));
        }
        if entry_meta.len() > MAX_ENTRY_BYTES {
            return Err(HostError::new(
                HostError::ENTRY_TOO_LARGE,
                format!("entry is {} bytes (max {MAX_ENTRY_BYTES})", entry_meta.len()),
            ));
        }
        let canonical_entry = fs::canonicalize(&entry)
            .map_err(|error| HostError::new(HostError::ENTRY_MISSING, error.to_string()))?;
        if !canonical_entry.starts_with(&canonical_dir) {
            return Err(HostError::new(HostError::PACKAGE, "entry escapes the module directory"));
        }

        let mut total = 0u64;
        let mut count = 0usize;
        scan(&canonical_dir, 0, &mut total, &mut count)?;

        Ok(Self {
            dir: canonical_dir,
            manifest,
            entry: canonical_entry,
            entry_bytes: entry_meta.len(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }

    pub fn entry_path(&self) -> &Path {
        &self.entry
    }

    pub fn entry_bytes(&self) -> u64 {
        self.entry_bytes
    }
}

fn scan(dir: &Path, depth: usize, total: &mut u64, count: &mut usize) -> HostResult<()> {
    if depth > MAX_PACKAGE_DEPTH {
        return Err(HostError::new(HostError::PACKAGE, "directory nesting is too deep"));
    }
    let entries = fs::read_dir(dir).map_err(|error| {
        HostError::new(HostError::PACKAGE, format!("read_dir {}: {error}", dir.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| HostError::new(HostError::PACKAGE, error.to_string()))?;
        *count += 1;
        if *count > MAX_PACKAGE_ENTRIES {
            return Err(HostError::new(HostError::PACKAGE, "too many entries"));
        }
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)
            .map_err(|error| HostError::new(HostError::PACKAGE, error.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(HostError::new(HostError::SYMLINK, path.display().to_string()));
        }
        if meta.is_dir() {
            scan(&path, depth + 1, total, count)?;
            continue;
        }
        if !meta.is_file() {
            return Err(HostError::new(HostError::PACKAGE, "unsupported file type"));
        }
        *total = total.saturating_add(meta.len());
        if *total > MAX_PACKAGE_BYTES {
            return Err(HostError::new(HostError::PACKAGE_TOO_LARGE, "package is too large"));
        }
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            let lower = extension.to_ascii_lowercase();
            // `.js` 只在 `ui/` 下放行（壳只把它当网页发；包根上的脚本仍按可执行拒）。
            let in_ui = path
                .components()
                .any(|part| part.as_os_str() == "ui");
            if EXECUTABLE_EXTENSIONS.contains(&lower.as_str()) && !(lower == "js" && in_ui) {
                return Err(HostError::new(
                    HostError::EXECUTABLE,
                    format!("native executable is not allowed: {}", path.display()),
                ));
            }
        }
    }
    Ok(())
}

/// 允许出现的后缀清单（给文档 / 验收对照用；**不是**白名单校验——数据文件可以是任意后缀，
/// 只有原生可执行与脚本被明确拒绝）。
pub fn forbidden_extensions() -> BTreeSet<&'static str> {
    EXECUTABLE_EXTENSIONS.iter().copied().collect()
}
