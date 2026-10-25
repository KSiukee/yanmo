//! 受控外部服务出口（第一刀）：**只允许公开 http(s) 的 GET**。
//!
//! 这一层在**宿主进程**里，核心/协议/壳仍然零出网。做法：
//! - 只有模块清单声明了 `network` 且用户授予、并且宿主收到 `--allow-network` 时，
//!   链接器里才有 `env.http_get`；
//! - URL 只收 http/https、拒绝带账号密码、拒绝回环/私网/链路本地地址；
//! - **不跟随重定向**（防止 302 到内网）；
//! - 单次响应有上限，超时 10 秒；
//! - 只返回正文，不暴露响应头/状态码；非 2xx 算失败。
//!
//! 还没做：域名白名单、DNS 重绑定防护、凭据保险箱、持久化访问日志、全局离线开关。

use std::io::{self, Read};
use std::sync::Arc;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use url::Url;

pub const MAX_URL_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkError {
    pub code: &'static str,
    pub detail: String,
}

impl NetworkError {
    pub const DENIED: &'static str = "host.network_denied";
    pub const FAILED: &'static str = "host.network_failed";
    pub const TOO_LARGE: &'static str = "host.network_too_large";

    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

/// 拉一个公开 URL 的正文（GET，不跟随重定向）。
pub fn fetch(url: &str, max_bytes: usize) -> Result<Vec<u8>, NetworkError> {
    fetch_inner(url, max_bytes, None)
}

/// 同 [`fetch`]，但带 `Authorization: Bearer`（秘密只在本函数内存在，不写日志）。
pub fn fetch_with_auth(
    url: &str,
    max_bytes: usize,
    bearer: &str,
) -> Result<Vec<u8>, NetworkError> {
    fetch_inner(url, max_bytes, Some(bearer))
}

fn fetch_inner(
    url: &str,
    max_bytes: usize,
    bearer: Option<&str>,
) -> Result<Vec<u8>, NetworkError> {
    let parsed = check_url(url)?;
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| NetworkError::new(NetworkError::FAILED, error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .resolver(PublicResolver)
        .tls_connector(Arc::new(connector))
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .user_agent("YanmoModule/0.129")
        .build();
    let mut request = agent.get(parsed.as_str());
    if let Some(bearer) = bearer {
        request = request.set("Authorization", &format!("Bearer {bearer}"));
    }
    let response = request.call().map_err(|error| match error {
        ureq::Error::Status(code, _) => {
            NetworkError::new(NetworkError::FAILED, format!("HTTP {code}"))
        }
        ureq::Error::Transport(error) => NetworkError::new(NetworkError::FAILED, error.to_string()),
    })?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|error| NetworkError::new(NetworkError::FAILED, error.to_string()))?;
    if body.len() > max_bytes {
        return Err(NetworkError::new(
            NetworkError::TOO_LARGE,
            format!("response exceeds {max_bytes} bytes"),
        ));
    }
    Ok(body)
}

/// 解析一次并**只返回全部公网的地址**；ureq 用这些地址连接，不再二次解析。
pub fn resolve_public(netloc: &str) -> io::Result<Vec<SocketAddr>> {
    let addresses: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
    if addresses.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no addresses"));
    }
    if addresses.iter().any(|address| ip_is_blocked(address.ip())) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "resolved address is not public",
        ));
    }
    Ok(addresses)
}

struct PublicResolver;

impl ureq::Resolver for PublicResolver {
    fn resolve(&self, netloc: &str) -> io::Result<Vec<SocketAddr>> {
        resolve_public(netloc)
    }
}

/// 只做**静态策略**检查；DNS 由 [`PublicResolver`] 解析并核对。
pub fn check_url(url: &str) -> Result<Url, NetworkError> {
    if url.len() > MAX_URL_BYTES {
        return Err(NetworkError::new(NetworkError::DENIED, "url is too long"));
    }
    let parsed =
        Url::parse(url).map_err(|error| NetworkError::new(NetworkError::DENIED, error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(NetworkError::new(
            NetworkError::DENIED,
            format!("scheme {} is not allowed", parsed.scheme()),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(NetworkError::new(
            NetworkError::DENIED,
            "credentials in url are not allowed",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| NetworkError::new(NetworkError::DENIED, "missing host"))?;
    if host_is_blocked(host) {
        return Err(NetworkError::new(
            NetworkError::DENIED,
            format!("host {host} is not allowed"),
        ));
    }
    Ok(parsed)
}

fn host_is_blocked(host: &str) -> bool {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower.ends_with(".lan")
        || lower.ends_with(".home")
    {
        return true;
    }
    match lower.parse::<IpAddr>() {
        Ok(ip) => ip_is_blocked(ip),
        Err(_) => false,
    }
}

fn ip_is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ipv4_is_blocked(ip),
        IpAddr::V6(ip) => ipv6_is_blocked(ip),
    }
}

fn ipv4_is_blocked(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || octets[0] >= 240
}

fn ipv6_is_blocked(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    let segments = ip.segments();
    (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || ip.to_ipv4_mapped().is_some_and(ipv4_is_blocked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localhost_resolution_is_rejected() {
        assert!(resolve_public("localhost:80").is_err());
        assert!(resolve_public("127.0.0.1:80").is_err());
    }
}
