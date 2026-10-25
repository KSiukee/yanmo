//! 模块子 WebView 的**只读自定义协议**：只从模块自己的 `ui/` 目录发文件。
//!
//! 不用 TCP/回环服务：Tauri 的 `moduleui://` 协议只在壳内可请求，不占端口、
//! 不出网。安全边界仍是：只读、路径逐段拒绝 `..`/盘符、规范化后必须仍在
//! `ui/` 下、响应带 CSP。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::http::Response;

const CSP: &str = "default-src 'self'; connect-src 'none'; img-src 'self' data:; \
media-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; \
font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// 当前模块的 `ui/` 根目录；同时只服务一个模块。
#[derive(Default)]
pub struct ModuleUiState {
    pub root: Arc<Mutex<Option<PathBuf>>>,
}

impl ModuleUiState {
    pub fn set(&self, root: PathBuf) {
        if let Ok(mut slot) = self.root.lock() {
            *slot = Some(root);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut slot) = self.root.lock() {
            *slot = None;
        }
    }
}

/// Tauri 自定义协议的处理函数：`path` 是 `moduleui://localhost/...` 的路径。
pub fn respond(state: &ModuleUiState, path: &str) -> Response<Vec<u8>> {
    let root = match state.root.lock() {
        Ok(slot) => match slot.as_ref() {
            Some(root) => root.clone(),
            None => return error(404, "not found"),
        },
        Err(_) => return error(500, "state poisoned"),
    };
    let Some(relative) = decode_relative(path) else {
        return error(403, "forbidden");
    };
    let Ok(canonical) = root.join(relative).canonicalize() else {
        return error(404, "not found");
    };
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return error(403, "forbidden");
    }
    let Ok(body) = fs::read(&canonical) else {
        return error(500, "read failed");
    };
    Response::builder()
        .status(200)
        .header("Content-Type", content_type(&canonical))
        .header("Content-Security-Policy", CSP)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .header("Referrer-Policy", "no-referrer")
        .body(body)
        .unwrap_or_else(|_| error(500, "response failed"))
}

fn error(status: u16, text: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Content-Security-Policy", CSP)
        .body(text.as_bytes().to_vec())
        .unwrap()
}

fn decode_relative(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim_start_matches('/');
    let raw = raw.split('?').next().unwrap_or("");
    let raw = if raw.is_empty() { "index.html" } else { raw };
    let mut decoded = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            decoded.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let text = String::from_utf8(decoded).ok()?;
    if text.contains('\\') || text.contains(':') || text.starts_with('/') {
        return None;
    }
    let mut path = PathBuf::new();
    for part in text.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()).unwrap_or_default() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_cannot_escape() {
        assert!(decode_relative("../secret").is_none());
        assert!(decode_relative("a/../../secret").is_none());
        assert!(decode_relative("C:secret").is_none());
        assert!(decode_relative("a%2F..%2Fsecret").is_none());
        assert_eq!(decode_relative("js/app.js").unwrap(), PathBuf::from("js/app.js"));
        assert_eq!(decode_relative("").unwrap(), PathBuf::from("index.html"));
    }

    #[test]
    fn serves_index_with_csp() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<h1>hi</h1>").unwrap();
        let state = ModuleUiState::default();
        state.set(dir.path().canonicalize().unwrap());
        let response = respond(&state, "/index.html");
        assert_eq!(response.status(), 200);
        assert!(response.headers().get("Content-Security-Policy").is_some());
        assert!(String::from_utf8_lossy(response.body()).contains("<h1>hi</h1>"));
    }
}
