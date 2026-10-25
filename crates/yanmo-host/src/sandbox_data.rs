//! `env.data_get/put/delete`：own_data 授予后，模块只能读写自己的 `mod_<id>_kv`。
//!
//! ABI：
//! - `data_get(key_ptr, key_len, out_ptr, out_cap) -> len | -1 | -2`
//!   - `-1`：没有这条 / 宿主错误；`-2`：缓冲区不够（可以加大重试）；
//! - `data_put(key_ptr, key_len, value_ptr, value_len) -> 0 | -1`
//! - `data_delete(key_ptr, key_len) -> 0 | -1`
//!
//! 指针/长度都按 guest 线性内存校验；值上限、单模块配额、频率与整库硬上限
//! 全在宿主侧 `ModuleData` 里执行，guest 拿不到 SQL。

use wasmi::{Caller, Extern, Linker};

use crate::error::{HostError, HostResult};
use crate::sandbox::HostState;

const MAX_KEY_BYTES: usize = 128;
const MAX_VALUE_BYTES: usize = 1024 * 1024;

pub(super) fn link_data(linker: &mut Linker<HostState>) -> HostResult<()> {
    linker
        .func_wrap(
            "env",
            "data_get",
            |mut caller: Caller<'_, HostState>, key_ptr: i32, key_len: i32, out_ptr: i32, out_cap: i32| -> i32 {
                if key_ptr < 0 || key_len < 0 || out_ptr < 0 || out_cap < 0 {
                    return -1;
                }
                let Some(key) = read_text(&mut caller, key_ptr, key_len) else {
                    return -1;
                };
                let Some(data) = caller.data().data.as_ref().cloned() else {
                    return -1;
                };
                let module_id = caller.data().module_id.clone();
                let value = {
                    let Ok(data) = data.lock() else { return -1 };
                    match data.kv_get(&module_id, &key) {
                        Ok(Some(value)) => value,
                        _ => return -1,
                    }
                };
                if value.len() > out_cap as usize {
                    return -2;
                }
                if write_bytes(&mut caller, out_ptr, &value).is_err() {
                    return -1;
                }
                value.len() as i32
            },
        )
        .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    linker
        .func_wrap(
            "env",
            "data_put",
            |mut caller: Caller<'_, HostState>, key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32| -> i32 {
                if key_ptr < 0 || key_len < 0 || value_ptr < 0 || value_len < 0 {
                    return -1;
                }
                let Some(key) = read_text(&mut caller, key_ptr, key_len) else {
                    return -1;
                };
                let Some(value) = read_bytes(&mut caller, value_ptr, value_len, MAX_VALUE_BYTES) else {
                    return -1;
                };
                let Some(data) = caller.data().data.as_ref().cloned() else {
                    return -1;
                };
                let module_id = caller.data().module_id.clone();
                let Ok(mut data) = data.lock() else { return -1 };
                match data.kv_put(&module_id, &key, &value) {
                    Ok(()) => 0,
                    Err(_) => -1,
                }
            },
        )
        .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    linker
        .func_wrap(
            "env",
            "data_delete",
            |mut caller: Caller<'_, HostState>, key_ptr: i32, key_len: i32| -> i32 {
                if key_ptr < 0 || key_len < 0 {
                    return -1;
                }
                let Some(key) = read_text(&mut caller, key_ptr, key_len) else {
                    return -1;
                };
                let Some(data) = caller.data().data.as_ref().cloned() else {
                    return -1;
                };
                let module_id = caller.data().module_id.clone();
                let Ok(mut data) = data.lock() else { return -1 };
                match data.kv_delete(&module_id, &key) {
                    Ok(_) => 0,
                    Err(_) => -1,
                }
            },
        )
        .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    Ok(())
}

fn read_text(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    if len == 0 || len as usize > MAX_KEY_BYTES {
        return None;
    }
    let bytes = read_bytes(caller, ptr, len, MAX_KEY_BYTES)?;
    String::from_utf8(bytes).ok()
}

fn read_bytes(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
    max: usize,
) -> Option<Vec<u8>> {
    if len < 0 || len as usize > max {
        return None;
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return None;
    };
    let mut buffer = vec![0u8; len as usize];
    memory.read(&*caller, ptr as usize, &mut buffer).ok()?;
    Some(buffer)
}

fn write_bytes(caller: &mut Caller<'_, HostState>, ptr: i32, bytes: &[u8]) -> Result<(), ()> {
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(());
    };
    memory
        .write(&mut *caller, ptr as usize, bytes)
        .map_err(|_| ())
}
