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

use std::fs;

use serde::Serialize;
use wasmi::{
    Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};

use crate::error::{HostError, HostResult};
use crate::package::ModulePackage;

/// 一次沙箱运行的预算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxConfig {
    /// 指令燃料（越大越晚停）
    pub fuel: u64,
    /// 线性内存上限（字节）
    pub max_memory_bytes: usize,
    /// 单条 `emit` 文本上限（字节）
    pub max_emit_bytes: usize,
    /// 全部 `emit` 文本合计上限（字节）
    pub max_total_emit_bytes: usize,
    /// 最多允许几条 `emit`
    pub max_emits: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            fuel: 10_000_000,
            max_memory_bytes: 16 * 1024 * 1024,
            max_emit_bytes: 64 * 1024,
            max_total_emit_bytes: 256 * 1024,
            max_emits: 64,
        }
    }
}

impl SandboxConfig {
    pub fn validate(&self) -> HostResult<()> {
        let valid = self.fuel > 0
            && self.max_memory_bytes > 0
            && self.max_emit_bytes > 0
            && self.max_emits > 0
            && self.max_total_emit_bytes >= self.max_emit_bytes;
        if valid {
            Ok(())
        } else {
            Err(HostError::new(HostError::CONFIG, "sandbox limits must be non-zero and consistent"))
        }
    }
}

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
    violation: Option<HostError>,
}

/// 跑一次模块：**只调 `run()`，只给 `env.emit`**。
pub fn run_package(package: &ModulePackage, config: SandboxConfig) -> HostResult<RunReport> {
    config.validate()?;
    let wasm = fs::read(package.entry_path())?;

    let mut wasmi_config = Config::default();
    wasmi_config.consume_fuel(true);
    let engine = Engine::new(&wasmi_config);
    let module = Module::new(&engine, &wasm)
        .map_err(|error| HostError::new(HostError::WASM_INVALID, error.to_string()))?;

    for import in module.imports() {
        if import.module() != "env" || import.name() != "emit" {
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
