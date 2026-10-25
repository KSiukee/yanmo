//! 凭据保险箱：**密钥只进系统钥匙串，模块拿不到明文**。
//!
//! 存储分两层：
//! - **秘密本身**：Windows 凭据管理器（服务名 [`SERVICE`]，账户 `module_id/name`）；
//! - **绑定元数据**：核心 `module_credentials` 表（模块、名字、允许的域名），**不存秘密**。
//!
//! 宿主代理按模块拿到的绑定清单取秘密，只把 `Authorization` 头发给匹配域名的请求；
//! guest 只能看到 URL/响应，看不到 key。

use serde::{Deserialize, Serialize};

/// Windows 凭据管理器里的服务名。
pub const SERVICE: &str = "app.yanmo.credentials";
/// 凭据名上限。
pub const MAX_NAME: usize = 64;
/// 允许域名上限。
pub const MAX_HOST: usize = 253;
/// 密钥上限（字节）。
pub const MAX_SECRET: usize = 8192;

/// 一条凭据绑定：名字 + 允许注入的域名（可 `*.example.com`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialSpec {
    pub name: String,
    pub host: String,
}

impl CredentialSpec {
    /// URL 的 host 是否匹配这条绑定。
    pub fn allows(&self, host: &str) -> bool {
        allows_host(&self.host, host)
    }
}

/// 保险箱错误（稳定码 + 技术细节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultError {
    pub code: &'static str,
    pub detail: String,
}

impl VaultError {
    pub const INVALID: &'static str = "vault.invalid";
    pub const KEYRING: &'static str = "vault.keyring";
    pub const NOT_FOUND: &'static str = "vault.not_found";
    pub const UNSUPPORTED: &'static str = "vault.unsupported";

    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

/// 写一把密钥（覆盖同名）。
pub fn store(module_id: &str, name: &str, secret: &str) -> Result<(), VaultError> {
    validate_name(name)?;
    if secret.is_empty()
        || secret.len() > MAX_SECRET
        || !secret.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(VaultError::new(
            VaultError::INVALID,
            "secret must be printable ASCII (no spaces/newlines)",
        ));
    }
    write_keyring(module_id, name, secret)
}

/// 读一把密钥；**只给宿主代理**，不经过任何面向 guest 的通道。
pub fn load(module_id: &str, name: &str) -> Result<String, VaultError> {
    validate_name(name)?;
    let secret = read_keyring(module_id, name)?;
    if secret.is_empty() || !secret.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(VaultError::new(
            VaultError::INVALID,
            "stored secret is not printable ASCII",
        ));
    }
    Ok(secret)
}

/// 删一把密钥；不存在不算错。
pub fn remove(module_id: &str, name: &str) -> Result<(), VaultError> {
    validate_name(name)?;
    delete_keyring(module_id, name)
}

/// 凭据名只允许字母数字与 `._-`，避免路径/分隔符歧义。
pub fn validate_name(name: &str) -> Result<(), VaultError> {
    if name.is_empty()
        || name.len() > MAX_NAME
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(VaultError::new(VaultError::INVALID, "credential name is invalid"));
    }
    Ok(())
}

/// 允许域名：小写、无协议/路径/端口；可写 `*.example.com`。
pub fn validate_host(host: &str) -> Result<(), VaultError> {
    let host = host.trim();
    if host.is_empty()
        || host.len() > MAX_HOST
        || host.contains("://")
        || host.contains('/')
        || host.contains(' ')
        || host.contains(':')
    {
        return Err(VaultError::new(VaultError::INVALID, "credential host is invalid"));
    }
    let bare = host.strip_prefix("*.").unwrap_or(host);
    if bare.is_empty() || bare.contains('*') || !bare.contains('.') {
        return Err(VaultError::new(
            VaultError::INVALID,
            "credential host must be a domain (or *.domain)",
        ));
    }
    Ok(())
}

/// 精确域名或 `*.suffix` 子域匹配；大小写不敏感。
pub fn allows_host(spec_host: &str, url_host: &str) -> bool {
    let spec = spec_host.trim_end_matches('.').to_ascii_lowercase();
    let url = url_host.trim_end_matches('.').to_ascii_lowercase();
    if let Some(suffix) = spec.strip_prefix("*.") {
        url.len() > suffix.len()
            && url.ends_with(suffix)
            && url.as_bytes()[url.len() - suffix.len() - 1] == b'.'
    } else {
        spec == url
    }
}

fn account(module_id: &str, name: &str) -> String {
    format!("{module_id}/{name}")
}

#[cfg(windows)]
fn entry(module_id: &str, name: &str) -> Result<keyring::Entry, VaultError> {
    keyring::Entry::new(SERVICE, &account(module_id, name))
        .map_err(|error| VaultError::new(VaultError::KEYRING, error.to_string()))
}

#[cfg(windows)]
fn write_keyring(module_id: &str, name: &str, secret: &str) -> Result<(), VaultError> {
    entry(module_id, name)?
        .set_password(secret)
        .map_err(|error| VaultError::new(VaultError::KEYRING, error.to_string()))
}

#[cfg(windows)]
fn read_keyring(module_id: &str, name: &str) -> Result<String, VaultError> {
    entry(module_id, name)?
        .get_password()
        .map_err(|error| match error {
            keyring::Error::NoEntry => VaultError::new(VaultError::NOT_FOUND, "credential not found"),
            other => VaultError::new(VaultError::KEYRING, other.to_string()),
        })
}

#[cfg(windows)]
fn delete_keyring(module_id: &str, name: &str) -> Result<(), VaultError> {
    match entry(module_id, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(VaultError::new(VaultError::KEYRING, error.to_string())),
    }
}

#[cfg(not(windows))]
fn write_keyring(_module_id: &str, _name: &str, _secret: &str) -> Result<(), VaultError> {
    Err(VaultError::new(
        VaultError::UNSUPPORTED,
        "system credential vault is only wired on Windows",
    ))
}

#[cfg(not(windows))]
fn read_keyring(_module_id: &str, _name: &str) -> Result<String, VaultError> {
    Err(VaultError::new(
        VaultError::UNSUPPORTED,
        "system credential vault is only wired on Windows",
    ))
}

#[cfg(not(windows))]
fn delete_keyring(_module_id: &str, _name: &str) -> Result<(), VaultError> {
    Err(VaultError::new(
        VaultError::UNSUPPORTED,
        "system credential vault is only wired on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_and_exact_hosts_match() {
        assert!(allows_host("api.example.com", "api.example.com"));
        assert!(allows_host("api.example.com", "API.EXAMPLE.COM"));
        assert!(!allows_host("api.example.com", "evil.example.com"));
        assert!(allows_host("*.example.com", "api.example.com"));
        assert!(allows_host("*.example.com", "a.b.example.com"));
        assert!(!allows_host("*.example.com", "example.com"));
        assert!(!allows_host("*.example.com", "notexample.com"));
    }

    #[test]
    fn names_and_hosts_are_narrow() {
        assert!(validate_name("openai").is_ok());
        assert!(validate_name("bad/name").is_err());
        assert!(validate_host("api.openai.com").is_ok());
        assert!(validate_host("*.example.com").is_ok());
        assert!(validate_host("https://api.openai.com").is_err());
        assert!(validate_host("api.openai.com/path").is_err());
        assert!(validate_host("api.openai.com:443").is_err());
        assert!(validate_host("localhost").is_err());
    }
}

#[cfg(all(test, windows))]
mod keyring_tests {
    use super::*;

    #[test]
    fn keyring_round_trip() {
        let module = "app.yanmo.vault.self_test";
        let name = "roundtrip";
        let _ = remove(module, name);
        store(module, name, "sk-test-1234567890").unwrap();
        assert_eq!(load(module, name).unwrap(), "sk-test-1234567890");
        remove(module, name).unwrap();
        assert_eq!(
            load(module, name).unwrap_err().code,
            VaultError::NOT_FOUND
        );
    }
}
