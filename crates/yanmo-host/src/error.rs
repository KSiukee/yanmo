//! 宿主错误：**稳定码 + 技术细节**（英文；界面文案等宿主界面那一层再翻）。
//!
//! 码是给日志、命令行与将来的管理页用的；`detail` 只给开发者看。

use std::fmt;

/// 宿主侧错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub code: &'static str,
    pub detail: String,
}

impl HostError {
    pub const PACKAGE: &'static str = "host.package_invalid";
    pub const MANIFEST: &'static str = "host.manifest_invalid";
    pub const ENTRY_MISSING: &'static str = "host.entry_missing";
    pub const ENTRY_NOT_WASM: &'static str = "host.entry_not_wasm";
    pub const ENTRY_TOO_LARGE: &'static str = "host.entry_too_large";
    pub const EXECUTABLE: &'static str = "host.package_has_executable";
    pub const SYMLINK: &'static str = "host.package_has_symlink";
    pub const PACKAGE_TOO_LARGE: &'static str = "host.package_too_large";
    pub const IO: &'static str = "host.io";
    pub const WASM_INVALID: &'static str = "host.wasm_invalid";
    pub const IMPORT_NOT_ALLOWED: &'static str = "host.import_not_allowed";
    pub const MISSING_RUN: &'static str = "host.missing_run";
    pub const CONFIG: &'static str = "host.config_invalid";
    pub const FUEL: &'static str = "host.fuel_exhausted";
    pub const TRAP: &'static str = "host.trap";
    pub const EMIT_TOO_LARGE: &'static str = "host.emit_too_large";
    pub const EMIT_LIMIT: &'static str = "host.emit_limit";
    pub const EMIT_INVALID: &'static str = "host.emit_invalid";
    pub const CAPABILITY_DENIED: &'static str = "host.capability_denied";
    pub const READ_LIMIT: &'static str = "host.read_limit";
    pub const READ_INVALID: &'static str = "host.read_invalid";
    pub const CORE: &'static str = "host.core_error";
    pub const LINK: &'static str = "host.link_failed";

    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for HostError {}

impl From<std::io::Error> for HostError {
    fn from(error: std::io::Error) -> Self {
        Self::new(Self::IO, error.to_string())
    }
}

pub type HostResult<T> = std::result::Result<T, HostError>;
