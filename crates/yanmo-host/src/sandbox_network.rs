//! `env.http_get` / `env.http_get_auth` 的宿主实现。
//!
//! 秘密只在这一层从系统钥匙串取出、放进请求头；不写日志、不返回 guest。
//! 单独成件：网络 ABI/配额/日志不跟 wasm 链接细节混在一个文件里。

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use wasmi::{Caller, Extern, Linker};

use crate::error::{HostError, HostResult};
use crate::network;
use crate::sandbox::HostState;

/// 把两个网络 import 挂到链接器上（只有 allow_network 时调用）。
pub(super) fn link_http(linker: &mut Linker<HostState>, allow_auth: bool) -> HostResult<()> {
    linker
        .func_wrap(
            "env",
            "http_get",
            |mut caller: Caller<'_, HostState>,
             url_ptr: i32,
             url_len: i32,
             out_ptr: i32,
             out_cap: i32|
             -> i32 { http_get(&mut caller, url_ptr, url_len, out_ptr, out_cap) },
        )
        .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    if allow_auth {
        linker
            .func_wrap(
                "env",
                "http_get_auth",
                |mut caller: Caller<'_, HostState>,
                 url_ptr: i32,
                 url_len: i32,
                 name_ptr: i32,
                 name_len: i32,
                 out_ptr: i32,
                 out_cap: i32|
                 -> i32 {
                    http_get_auth(
                        &mut caller, url_ptr, url_len, name_ptr, name_len, out_ptr, out_cap,
                    )
                },
            )
            .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    }
    Ok(())
}

pub(super) fn http_get(
    caller: &mut Caller<'_, HostState>,
    url_ptr: i32,
    url_len: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    let Some(url) = read_string(caller, url_ptr, url_len, network::MAX_URL_BYTES) else {
        return -1;
    };
    if quota_left(caller) == false {
        return -1;
    }
    let host = match network::check_url(&url) {
        Ok(parsed) => parsed.host_str().unwrap_or("?").to_string(),
        Err(error) => {
            log_denied(caller, "?", &error.code, &error.detail);
            audit(caller, "?", 0, None, error.code);
            return count_failure(caller);
        }
    };
    if !host_allowed(caller, &host) {
        log_denied(caller, &host, "host.network_denied", "host is not in the allowlist");
        audit(caller, &host, 0, None, "host.network_denied");
        return count_failure(caller);
    }
    let max = caller.data().max_network_bytes;
    match network::fetch(&url, max) {
        Ok(body) => write_body(caller, out_ptr, out_cap, &body, &host, None),
        Err(error) => {
            log_denied(caller, &host, error.code, &error.detail);
            audit(caller, &host, 0, None, error.code);
            count_failure(caller)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn http_get_auth(
    caller: &mut Caller<'_, HostState>,
    url_ptr: i32,
    url_len: i32,
    name_ptr: i32,
    name_len: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    let Some(url) = read_string(caller, url_ptr, url_len, network::MAX_URL_BYTES) else {
        return -1;
    };
    let Some(name) = read_string(caller, name_ptr, name_len, 64) else {
        return -1;
    };
    if !quota_left(caller) {
        return -1;
    }
    let Some(spec) = caller
        .data()
        .credentials
        .iter()
        .find(|spec| spec.name == name)
        .cloned()
    else {
        log_denied(caller, "?", "vault.not_bound", "credential is not bound to this module");
        audit(caller, "?", 0, Some(&name), "vault.not_bound");
        return count_failure(caller);
    };
    let parsed = match network::check_url(&url) {
        Ok(parsed) => parsed,
        Err(error) => {
            log_denied(caller, &spec.host, &error.code, &error.detail);
            audit(caller, &spec.host, 0, Some(&name), error.code);
            return count_failure(caller);
        }
    };
    let Some(host) = parsed.host_str() else {
        return count_failure(caller);
    };
    if !host_allowed(caller, host) {
        log_denied(caller, host, "host.network_denied", "host is not in the allowlist");
        audit(caller, host, 0, Some(&name), "host.network_denied");
        return count_failure(caller);
    }
    if !spec.allows(host) {
        log_denied(
            caller,
            host,
            "vault.host_mismatch",
            &format!("credential {} is bound to {}", spec.name, spec.host),
        );
        audit(caller, host, 0, Some(&name), "vault.host_mismatch");
        return count_failure(caller);
    }
    let module_id = caller.data().module_id.clone();
    let secret = match yanmo_vault::load(&module_id, &name) {
        Ok(secret) => secret,
        Err(error) => {
            log_denied(caller, host, error.code, &error.detail);
            audit(caller, host, 0, Some(&name), error.code);
            return count_failure(caller);
        }
    };
    let max = caller.data().max_network_bytes;
    let result = network::fetch_with_auth(&url, max, &secret);
    drop(secret);
    match result {
        Ok(body) => write_body(caller, out_ptr, out_cap, &body, host, Some(&name)),
        Err(error) => {
            log_denied(caller, host, error.code, &error.detail);
            audit(caller, host, 0, Some(&name), error.code);
            count_failure(caller)
        }
    }
}

fn read_string(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
    max: usize,
) -> Option<String> {
    if ptr < 0 || len <= 0 || len as usize > max {
        return None;
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return None;
    };
    let mut bytes = vec![0u8; len as usize];
    memory.read(&*caller, ptr as usize, &mut bytes).ok()?;
    String::from_utf8(bytes).ok()
}

fn host_allowed(caller: &Caller<'_, HostState>, host: &str) -> bool {
    let allowed = &caller.data().allowed_hosts;
    allowed.is_empty() || allowed.iter().any(|spec| yanmo_vault::allows_host(spec, host))
}

fn quota_left(caller: &Caller<'_, HostState>) -> bool {
    caller.data().network_requests < caller.data().max_network_requests
}

fn count_failure(caller: &mut Caller<'_, HostState>) -> i32 {
    caller.data_mut().network_requests += 1;
    -1
}

fn write_body(
    caller: &mut Caller<'_, HostState>,
    out_ptr: i32,
    out_cap: i32,
    body: &[u8],
    host: &str,
    credential: Option<&str>,
) -> i32 {
    if out_ptr < 0 || out_cap < 0 {
        audit(caller, host, 0, credential, "host.bad_output");
        return count_failure(caller);
    }
    if body.len() > out_cap as usize {
        audit(caller, host, 0, credential, "host.output_too_small");
        return count_failure(caller);
    }
    if caller
        .data()
        .network_bytes
        .saturating_add(body.len())
        > caller.data().max_total_network_bytes
    {
        audit(caller, host, 0, credential, "host.network_quota");
        return count_failure(caller);
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        audit(caller, host, 0, credential, "host.memory_missing");
        return count_failure(caller);
    };
    if memory.write(&mut *caller, out_ptr as usize, body).is_err() {
        audit(caller, host, 0, credential, "host.memory_write");
        return count_failure(caller);
    }
    let credential_owned = credential.map(str::to_string);
    let state = caller.data_mut();
    state.network_requests += 1;
    state.network_bytes = state.network_bytes.saturating_add(body.len());
    let credential = credential.unwrap_or("-");
    eprintln!(
        "network module={} host={} bytes={} credential={credential}",
        state.module_id,
        host,
        body.len()
    );
    audit(caller, host, body.len(), credential_owned.as_deref(), "ok");
    body.len() as i32
}

fn log_denied(caller: &mut Caller<'_, HostState>, host: &str, code: &str, detail: &str) {
    let module_id = caller.data().module_id.clone();
    eprintln!("network module={module_id} host={host} result={code} detail={detail}");
}

fn audit(
    caller: &mut Caller<'_, HostState>,
    host: &str,
    bytes: usize,
    credential: Option<&str>,
    result: &str,
) {
    let Some(path) = caller.data().network_log.clone() else {
        return;
    };
    let module = caller.data().module_id.clone();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    const MAX_AUDIT_BYTES: u64 = 1024 * 1024;
    if fs::metadata(&path)
        .map(|meta| meta.len() >= MAX_AUDIT_BYTES)
        .unwrap_or(false)
    {
        let rotated = path.with_extension("1.jsonl");
        let _ = fs::rename(&path, rotated);
    }
    let line = serde_json::json!({
        "ts": ts,
        "module": module,
        "host": host,
        "bytes": bytes,
        "credential": credential,
        "result": result,
    })
    .to_string();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}
