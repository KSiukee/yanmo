//! `env.read` 的宿主实现：能力 id → 核心只读数据；单独成件以守住 sandbox 体积。

use std::sync::atomic::Ordering;

use wasmi::{Caller, Extern};

use crate::error::HostError;
use crate::sandbox::HostState;
use yanmo_proto::capability::CoreDataArea;
use yanmo_proto::ipc::IpcError;

fn fail_read(caller: &mut Caller<'_, HostState>, code: &'static str, detail: &str) -> i32 {
    caller.data_mut().violation = Some(HostError::new(code, detail));
    -1
}

/// 稳定的能力 id（**已发布，别改**）：1 作品 / 2 目录 / 3 正文 / 4 大纲 /
/// 5 碎片 / 6 设定卡 / 7 场景卡 / 8 伏笔。
pub(super) fn area_from_id(id: i32) -> Option<CoreDataArea> {
    match id {
        1 => Some(CoreDataArea::Works),
        2 => Some(CoreDataArea::Tree),
        3 => Some(CoreDataArea::Body),
        4 => Some(CoreDataArea::Outline),
        5 => Some(CoreDataArea::Fragments),
        6 => Some(CoreDataArea::Entities),
        7 => Some(CoreDataArea::Scenes),
        8 => Some(CoreDataArea::Foreshadows),
        _ => None,
    }
}

/// guest ABI：`read(cap_id, arg_ptr, arg_len, out_ptr, out_cap) -> len | -1 | -2`。
///
/// - `-1`：宿主记下违规（整次运行失败）；
/// - `-2`：结果比 `out_cap` 大 —— **这是缓冲区协商，不是违规**，guest 可以加大缓冲重试；
/// - 其它负数不会出现。
#[allow(clippy::too_many_arguments)]
pub(super) fn read(
    caller: &mut Caller<'_, HostState>,
    cap: i32,
    arg_ptr: i32,
    arg_len: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    if caller
        .data()
        .cancel
        .as_ref()
        .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
    {
        return fail_read(caller, HostError::STOPPED, "module was stopped");
    }
    let Some(area) = area_from_id(cap) else {
        return fail_read(caller, HostError::READ_INVALID, "unknown capability id");
    };
    {
        let state = caller.data();
        if !state.declared.contains(&area) {
            return fail_read(caller, HostError::CAPABILITY_DENIED, area.as_str());
        }
        if state.reads >= state.max_reads {
            return fail_read(caller, HostError::READ_LIMIT, "too many reads");
        }
    }
    if arg_ptr < 0 || arg_len < 0 || out_ptr < 0 || out_cap < 0 {
        return fail_read(caller, HostError::READ_INVALID, "negative pointer or length");
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return fail_read(caller, HostError::READ_INVALID, "module has no exported memory");
    };

    let arg = if arg_len == 0 {
        None
    } else {
        let length = arg_len as usize;
        if length > caller.data().max_read_arg_bytes {
            return fail_read(caller, HostError::READ_LIMIT, "read argument is too large");
        }
        let mut buffer = vec![0u8; length];
        if memory.read(&*caller, arg_ptr as usize, &mut buffer).is_err() {
            return fail_read(caller, HostError::READ_INVALID, "read argument is out of bounds");
        }
        match String::from_utf8(buffer) {
            Ok(text) => Some(text),
            Err(_) => return fail_read(caller, HostError::READ_INVALID, "read argument is not UTF-8"),
        }
    };

    let core = {
        let state = caller.data_mut();
        state.reads += 1;
        state.core.clone()
    };
    let Some(core) = core else {
        return fail_read(caller, HostError::CORE, "no core link");
    };
    let result = match core.lock() {
        Ok(mut client) => client.read(area, arg.as_deref()),
        Err(_) => Err(IpcError::new(
            IpcError::HANDSHAKE,
            "core link lock poisoned",
        )),
    };
    let data = match result {
        Ok(data) => data,
        Err(error) => {
            return fail_read(
                caller,
                HostError::CORE,
                &format!("{}: {}", error.code, error.detail),
            );
        }
    };
    if data.len() > caller.data().max_read_result_bytes {
        return fail_read(caller, HostError::READ_LIMIT, "core result is too large");
    }
    if data.len() > out_cap as usize {
        return -2;
    }
    if memory.write(&mut *caller, out_ptr as usize, data.as_bytes()).is_err() {
        return fail_read(caller, HostError::READ_INVALID, "result write is out of bounds");
    }
    data.len() as i32
}
