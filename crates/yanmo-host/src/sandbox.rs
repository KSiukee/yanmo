//! WASM 沙箱：**没有 WASI、没有文件、没有网络；能力就是 import 白名单**。
//!
//! # 唯一入口
//!
//! 宿主只调 guest 的一个导出：`run() -> i32`。没有第二个调用点，也没有"把宿主对象
//! 交给插件"的通道——guest 能看到的只有两个数字参数（`emit` 的指针与长度）与自己的线性内存。
//!
//! # 能力白名单
//!
//! 现在只开放一个 import：`env.emit(ptr, len)`（把一段 UTF-8 文本交给宿主）。
//! 任何别的 import（`wasi_snapshot_preview1.*`、`env.read_file`、……）在实例化前就拒。
//! 这不是"运行时不调用"，而是**链接器里根本没有那个函数**。
//!
//! # 预算
//!
//! - **fuel**：指令级计量，死循环会被中断（`host.fuel_exhausted`）；
//! - **内存**：`StoreLimits` 限制线性内存上限，`memory.grow` 超限失败；
//! - **输出**：单条 / 总字节 / 条数都有上限，超了整次运行失败（不截断冒充成功）。

use std::collections::BTreeSet;
use std::fs;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use wasmi::{
    Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};
use yanmo_proto::capability::CoreDataArea;
use yanmo_proto::ipc::IpcError;
use yanmo_ipc::Client;

use crate::error::{HostError, HostResult};
use crate::core_link::CoreLink;
use crate::package::ModulePackage;
use crate::sandbox_config::SandboxConfig;

/// 一次运行的结局（guest 自己返回非零也算"跑完了"，只是它报告了错误码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    GuestError { code: i32 },
}

/// 一次运行的报告（命令行与管理页都看它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunReport {
    pub module_id: String,
    pub status: RunStatus,
    pub emitted: Vec<String>,
    pub fuel_used: u64,
    /// 跑完之后线性内存的实际大小（受 `max_memory_bytes` 限制）
    pub memory_bytes: u64,
}

struct HostState {
    limits: StoreLimits,
    emitted: Vec<String>,
    emitted_bytes: usize,
    max_emit_bytes: usize,
    max_total_emit_bytes: usize,
    max_emits: usize,
    core: Option<Arc<Mutex<CoreLink>>>,
    declared: BTreeSet<CoreDataArea>,
    max_read_arg_bytes: usize,
    max_read_result_bytes: usize,
    max_reads: usize,
    reads: usize,
    violation: Option<HostError>,
}

/// 跑一次模块（**不接核心**）：只调 `run()`，只给 `env.emit`。
pub fn run_package(package: &ModulePackage, config: SandboxConfig) -> HostResult<RunReport> {
    run_package_with_core(package, config, None)
}

/// 跑一次模块，可选接上核心 IPC。
///
/// **`read` import 只在两个条件同时成立时存在**：模块清单声明了至少一块只读能力，
/// 且宿主真的连上了核心。否则链接器里没有 `env.read`——模块写不出来，也就调不到。
pub fn run_package_with_core(
    package: &ModulePackage,
    config: SandboxConfig,
    core: Option<Client>,
) -> HostResult<RunReport> {
    let core = core.map(|client| Arc::new(Mutex::new(CoreLink::from_client(client))));
    run_package_with_core_shared(package, config, core)
}

/// 同 [`run_package_with_core`]，但核心连接可以**跨多次运行复用**（常驻宿主用）。
pub fn run_package_with_core_shared(
    package: &ModulePackage,
    config: SandboxConfig,
    core: Option<Arc<Mutex<CoreLink>>>,
) -> HostResult<RunReport> {
    config.validate()?;
    let declared: BTreeSet<CoreDataArea> =
        package.manifest().capabilities.read.iter().copied().collect();
    let allow_read = core.is_some() && !declared.is_empty();
    let wasm = fs::read(package.entry_path())?;

    let mut wasmi_config = Config::default();
    wasmi_config.consume_fuel(true);
    let engine = Engine::new(&wasmi_config);
    let module = Module::new(&engine, &wasm)
        .map_err(|error| HostError::new(HostError::WASM_INVALID, error.to_string()))?;

    for import in module.imports() {
        let allowed = import.module() == "env"
            && (import.name() == "emit" || (import.name() == "read" && allow_read));
        if !allowed {
            return Err(HostError::new(
                HostError::IMPORT_NOT_ALLOWED,
                format!("{}::{}", import.module(), import.name()),
            ));
        }
    }

    let limits = StoreLimitsBuilder::new()
        .memory_size(config.max_memory_bytes)
        .build();
    let state = HostState {
        limits,
        emitted: Vec::new(),
        emitted_bytes: 0,
        max_emit_bytes: config.max_emit_bytes,
        max_total_emit_bytes: config.max_total_emit_bytes,
        max_emits: config.max_emits,
        core,
        declared,
        max_read_arg_bytes: config.max_read_arg_bytes,
        max_read_result_bytes: config.max_read_result_bytes,
        max_reads: config.max_reads,
        reads: 0,
        violation: None,
    };
    let mut store = Store::new(&engine, state);
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(config.fuel)
        .map_err(|error| HostError::new(HostError::FUEL, error.to_string()))?;

    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "env",
            "emit",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 { emit(&mut caller, ptr, len) },
        )
        .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    if allow_read {
        linker
            .func_wrap(
                "env",
                "read",
                |mut caller: Caller<'_, HostState>,
                 cap: i32,
                 arg_ptr: i32,
                 arg_len: i32,
                 out_ptr: i32,
                 out_cap: i32|
                 -> i32 { read(&mut caller, cap, arg_ptr, arg_len, out_ptr, out_cap) },
            )
            .map_err(|error| HostError::new(HostError::LINK, error.to_string()))?;
    }

    let pre = linker
        .instantiate(&mut store, &module)
        .map_err(|error| HostError::new(HostError::TRAP, error.to_string()))?;
    let instance = pre
        .start(&mut store)
        .map_err(|error| HostError::new(HostError::TRAP, error.to_string()))?;
    let run = instance
        .get_typed_func::<(), i32>(&store, "run")
        .map_err(|error| HostError::new(HostError::MISSING_RUN, error.to_string()))?;

    let fuel_before = store.get_fuel().unwrap_or(0);
    let call = run.call(&mut store, ());
    let fuel_after = store.get_fuel().unwrap_or(0);
    let fuel_used = fuel_before.saturating_sub(fuel_after);

    if let Some(error) = store.data().violation.clone() {
        return Err(error);
    }
    let status = match call {
        Ok(0) => RunStatus::Ok,
        Ok(code) => RunStatus::GuestError { code },
        Err(error) => return Err(map_wasmi_error(error)),
    };
    let memory_bytes = instance
        .get_export(&store, "memory")
        .and_then(Extern::into_memory)
        .map(|memory| memory.data_size(&store) as u64)
        .unwrap_or(0);
    let emitted = store.into_data().emitted;
    Ok(RunReport {
        module_id: package.manifest().id.clone(),
        status,
        emitted,
        fuel_used,
        memory_bytes,
    })
}

fn emit(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> i32 {
    if ptr < 0 || len < 0 {
        return fail(caller, HostError::EMIT_INVALID, "negative pointer or length");
    }
    let len = len as usize;
    if len > caller.data().max_emit_bytes {
        return fail(caller, HostError::EMIT_TOO_LARGE, "single emit is too large");
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return fail(caller, HostError::EMIT_INVALID, "module has no exported memory");
    };
    let mut buffer = vec![0u8; len];
    if memory.read(&*caller, ptr as usize, &mut buffer).is_err() {
        return fail(caller, HostError::EMIT_INVALID, "emit reads out of bounds");
    }
    let Ok(text) = String::from_utf8(buffer) else {
        return fail(caller, HostError::EMIT_INVALID, "emit is not valid UTF-8");
    };
    let state = caller.data_mut();
    if state.emitted.len() >= state.max_emits {
        return fail_from_state(state, HostError::EMIT_LIMIT, "too many emits");
    }
    if state.emitted_bytes.saturating_add(text.len()) > state.max_total_emit_bytes {
        return fail_from_state(state, HostError::EMIT_LIMIT, "total emit budget exceeded");
    }
    state.emitted_bytes += text.len();
    state.emitted.push(text);
    0
}

fn fail(caller: &mut Caller<'_, HostState>, code: &'static str, detail: &str) -> i32 {
    fail_from_state(caller.data_mut(), code, detail)
}

/// 稳定的能力 id（**已发布，别改**）：1 作品 / 2 目录 / 3 正文 / 4 大纲 /
/// 5 碎片 / 6 设定卡 / 7 场景卡 / 8 伏笔。
fn area_from_id(id: i32) -> Option<CoreDataArea> {
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
fn read(
    caller: &mut Caller<'_, HostState>,
    cap: i32,
    arg_ptr: i32,
    arg_len: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    let Some(area) = area_from_id(cap) else {
        return fail(caller, HostError::READ_INVALID, "unknown capability id");
    };
    {
        let state = caller.data();
        if !state.declared.contains(&area) {
            return fail(caller, HostError::CAPABILITY_DENIED, area.as_str());
        }
        if state.reads >= state.max_reads {
            return fail(caller, HostError::READ_LIMIT, "too many reads");
        }
    }
    if arg_ptr < 0 || arg_len < 0 || out_ptr < 0 || out_cap < 0 {
        return fail(caller, HostError::READ_INVALID, "negative pointer or length");
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return fail(caller, HostError::READ_INVALID, "module has no exported memory");
    };

    let arg = if arg_len == 0 {
        None
    } else {
        let length = arg_len as usize;
        if length > caller.data().max_read_arg_bytes {
            return fail(caller, HostError::READ_LIMIT, "read argument is too large");
        }
        let mut buffer = vec![0u8; length];
        if memory.read(&*caller, arg_ptr as usize, &mut buffer).is_err() {
            return fail(caller, HostError::READ_INVALID, "read argument is out of bounds");
        }
        match String::from_utf8(buffer) {
            Ok(text) => Some(text),
            Err(_) => return fail(caller, HostError::READ_INVALID, "read argument is not UTF-8"),
        }
    };

    let core = {
        let state = caller.data_mut();
        state.reads += 1;
        state.core.clone()
    };
    let Some(core) = core else {
        return fail(caller, HostError::CORE, "no core link");
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
            return fail(
                caller,
                HostError::CORE,
                &format!("{}: {}", error.code, error.detail),
            );
        }
    };
    if data.len() > caller.data().max_read_result_bytes {
        return fail(caller, HostError::READ_LIMIT, "core result is too large");
    }
    if data.len() > out_cap as usize {
        return -2;
    }
    if memory.write(&mut *caller, out_ptr as usize, data.as_bytes()).is_err() {
        return fail(caller, HostError::READ_INVALID, "result write is out of bounds");
    }
    data.len() as i32
}

fn fail_from_state(state: &mut HostState, code: &'static str, detail: &str) -> i32 {
    state.violation = Some(HostError::new(code, detail));
    -1
}

fn map_wasmi_error(error: wasmi::Error) -> HostError {
    if error.as_trap_code() == Some(wasmi::core::TrapCode::OutOfFuel) {
        HostError::new(HostError::FUEL, "fuel exhausted")
    } else {
        HostError::new(HostError::TRAP, error.to_string())
    }
}
