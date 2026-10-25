//! 模块子 WebView 的只读协议 + **受限 RPC 桥**。
//!
//! 传输仍走 Tauri 自定义协议 `moduleui://`（不占端口、不出网）。协议处理分两种：
//! - GET：只从当前模块 `ui/` 发静态文件；另带一个内置 `/_yanmo.js` 客户端；
//! - POST `/_rpc`：校验本次 UI 的随机 token，只允许**当前模块**的
//!   `module.info` / `module.status` / `module.run` / `module.stop`。
//!
//! 桥不读稿件、不暴露任意模块 id，也没有文件/网络能力。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::http::Response;
use tauri::{AppHandle, Manager};

use crate::commands::module_run::{start_module, status_module, stop_module, ModuleRuns};
use crate::storage::AppData;
use yanmo_core::store::Store;

const CSP: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data:; \
media-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; \
font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

const CLIENT_JS: &str = r#"
(() => {
  const token = new URLSearchParams(location.search).get("token") || "";
  async function rpc(method, params = {}) {
    const response = await fetch("/_rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ token, method, params }),
    });
    const data = await response.json();
    if (!data.ok) {
      const error = new Error(data.error?.code || "module_ui.rpc_failed");
      error.code = data.error?.code;
      throw error;
    }
    return data.result;
  }
  window.yanmo = {
    rpc,
    info: () => rpc("module.info"),
    status: () => rpc("module.status"),
    run: () => rpc("module.run"),
    stop: () => rpc("module.stop"),
  };
})();
"#;

struct Session {
    root: PathBuf,
    module_id: String,
    token: String,
}

/// 当前模块的 UI 会话；同一时间只服务一个模块。
#[derive(Clone, Default)]
pub struct ModuleUiState {
    session: Arc<Mutex<Option<Session>>>,
}

impl ModuleUiState {
    pub fn set(&self, root: PathBuf, module_id: String, token: String) {
        if let Ok(mut slot) = self.session.lock() {
            *slot = Some(Session { root, module_id, token });
        }
    }

    pub fn clear(&self) {
        if let Ok(mut slot) = self.session.lock() {
            *slot = None;
        }
    }

    /// 只清自己那一场会话：旧窗口的 Destroyed 事件不能把新会话清掉。
    pub fn clear_if_token(&self, token: &str) {
        if let Ok(mut slot) = self.session.lock() {
            if slot.as_ref().is_some_and(|session| session.token == token) {
                *slot = None;
            }
        }
    }
}

/// Tauri 自定义协议入口：GET 静态文件 / POST RPC。
pub fn respond(
    app: &AppHandle,
    state: &ModuleUiState,
    request: tauri::http::Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let path = request.uri().path().to_string();
    if request.method().as_str() == "POST" && path == "/_rpc" {
        return rpc(app, state, request.body());
    }
    if request.method().as_str() != "GET" {
        return error(405, "method not allowed");
    }
    if path == "/_yanmo.js" {
        return Response::builder()
            .status(200)
            .header("Content-Type", "text/javascript; charset=utf-8")
            .header("Content-Security-Policy", CSP)
            .header("Cache-Control", "no-store")
            .body(CLIENT_JS.as_bytes().to_vec())
            .unwrap();
    }
    static_file(state, &path)
}

fn rpc(app: &AppHandle, state: &ModuleUiState, body: &[u8]) -> Response<Vec<u8>> {
    if body.len() > 64 * 1024 {
        return json_response(413, false, json!({ "error": { "code": "module_ui.rpc_too_large" } }));
    }
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return json_response(400, false, json!({ "error": { "code": "module_ui.rpc_invalid" } }));
    };
    let token = value.get("token").and_then(Value::as_str).unwrap_or_default();
    let method = value.get("method").and_then(Value::as_str).unwrap_or_default();
    let module_id = {
        let Ok(slot) = state.session.lock() else {
            return json_response(500, false, json!({ "error": { "code": "module_ui.rpc_failed" } }));
        };
        let Some(session) = slot.as_ref() else {
            return json_response(409, false, json!({ "error": { "code": "module_ui.no_session" } }));
        };
        if token.is_empty() || token != session.token {
            return json_response(403, false, json!({ "error": { "code": "module_ui.rpc_denied" } }));
        }
        session.module_id.clone()
    };
    match dispatch(app, &module_id, method) {
        Ok(result) => json_response(200, true, json!({ "result": result })),
        Err(error) => json_response(200, false, json!({ "error": error })),
    }
}

fn dispatch(app: &AppHandle, module_id: &str, method: &str) -> Result<Value, Value> {
    let data = app.state::<AppData>();
    let runs = app.state::<ModuleRuns>();
    match method {
        "module.info" => {
            let card = data
                .with_store(|store: &mut Store| store.module_card(module_id))
                .map_err(api_error)?;
            serde_json::to_value(card).map_err(|_| json!({ "code": "module_ui.rpc_failed" }))
        }
        "module.status" => {
            let status = status_module(runs.inner(), module_id).map_err(api_error)?;
            serde_json::to_value(status).map_err(|_| json!({ "code": "module_ui.rpc_failed" }))
        }
        "module.run" => {
            let started = start_module(data.inner(), runs.inner(), module_id).map_err(api_error)?;
            serde_json::to_value(started).map_err(|_| json!({ "code": "module_ui.rpc_failed" }))
        }
        "module.stop" => {
            stop_module(runs.inner(), module_id).map_err(api_error)?;
            Ok(json!({}))
        }
        _ => Err(json!({ "code": "module_ui.unknown_method", "detail": method })),
    }
}

fn api_error(error: crate::error::ApiError) -> Value {
    json!({
        "code": error.code,
        "params": error.params,
        "detail": error.detail,
    })
}

fn static_file(state: &ModuleUiState, path: &str) -> Response<Vec<u8>> {
    let root = {
        let Ok(slot) = state.session.lock() else {
            return error(500, "state poisoned");
        };
        let Some(session) = slot.as_ref() else {
            return error(404, "not found");
        };
        session.root.clone()
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

fn json_response(status: u16, ok: bool, mut payload: Value) -> Response<Vec<u8>> {
    if let Value::Object(map) = &mut payload {
        map.insert("ok".to_string(), Value::Bool(ok));
    } else {
        payload = json!({ "ok": ok });
    }
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Content-Security-Policy", CSP)
        .header("Cache-Control", "no-store")
        .body(serde_json::to_vec(&payload).unwrap_or_default())
        .unwrap()
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
    fn static_file_has_csp_and_serves_index() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<h1>hi</h1>").unwrap();
        let state = ModuleUiState::default();
        state.set(
            dir.path().canonicalize().unwrap(),
            "app.example.x".to_string(),
            "token".to_string(),
        );
        let response = static_file(&state, "/index.html");
        assert_eq!(response.status(), 200);
        assert!(response.headers().get("Content-Security-Policy").is_some());
        assert!(String::from_utf8_lossy(response.body()).contains("<h1>hi</h1>"));
    }

    #[test]
    fn rpc_requires_the_session_token() {
        let state = ModuleUiState::default();
        state.set(
            PathBuf::from("ui"),
            "app.example.x".to_string(),
            "secret-token".to_string(),
        );
        let session = state.session.lock().unwrap();
        let session = session.as_ref().unwrap();
        assert_eq!(session.token, "secret-token");
        assert_eq!(session.module_id, "app.example.x");
    }
}
