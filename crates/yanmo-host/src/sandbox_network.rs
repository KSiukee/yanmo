//! `env.http_get` 的宿主实现：从 guest 内存读 URL、走受控代理、把正文写回。
//!
//! 单独成件：这是网络 ABI/配额/日志，不跟 wasm 链接细节混在一个文件里。

use wasmi::{Caller, Extern, Linker};

use crate::error::{HostError, HostResult};
use crate::network;
use crate::sandbox::HostState;

/// 把 `env.http_get` 挂到链接器上（只有 allow_network 时调用）。
pub(super) fn link_http(linker: &mut Linker<HostState>) -> HostResult<()> {
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
    Ok(())
}

pub(super) fn http_get(
    caller: &mut Caller<'_, HostState>,
    url_ptr: i32,
    url_len: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    if url_ptr < 0 || url_len < 0 || out_ptr < 0 || out_cap < 0 {
        return -1;
    }
    let url_len = url_len as usize;
    if url_len == 0 || url_len > network::MAX_URL_BYTES {
        return -1;
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return -1;
    };
    let mut url_bytes = vec![0u8; url_len];
    if memory
        .read(&*caller, url_ptr as usize, &mut url_bytes)
        .is_err()
    {
        return -1;
    }
    let Ok(url) = String::from_utf8(url_bytes) else {
        return -1;
    };
    let (module_id, requests, max_requests) = {
        let state = caller.data();
        (state.module_id.clone(), state.network_requests, state.max_network_requests)
    };
    if requests >= max_requests {
        return -1;
    }
    let host = network::check_url(&url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_else(|| "?".to_string());
    let max_bytes = caller.data().max_network_bytes;
    let body = match network::fetch(&url, max_bytes) {
        Ok(body) => body,
        Err(error) => {
            eprintln!(
                "network module={module_id} host={host} result={} detail={}",
                error.code, error.detail
            );
            caller.data_mut().network_requests += 1;
            return -1;
        }
    };
    if body.len() > out_cap as usize {
        caller.data_mut().network_requests += 1;
        return -2;
    }
    if memory
        .write(&mut *caller, out_ptr as usize, &body)
        .is_err()
    {
        return -1;
    }
    let state = caller.data_mut();
    state.network_requests += 1;
    state.network_bytes = state.network_bytes.saturating_add(body.len());
    eprintln!(
        "network module={} host={} bytes={}",
        state.module_id,
        host,
        body.len()
    );
    body.len() as i32
}
