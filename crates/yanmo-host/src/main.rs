//! 命令行：把一个模块目录交给沙箱跑一次，打印 JSON 报告。
//!
//! ```text
//! yanmo-host --module <目录> [--check] [--fuel N] [--max-memory BYTES]
//!            [--max-emit BYTES] [--max-total-emit BYTES] [--max-emits N]
//! ```
//!
//! `--check` 只做静态校验（清单 / 入口 / 包内文件），不执行载荷。
//! 这个二进制是**独立进程**：核心与壳将来通过协议拉起它，而不是把沙箱链进自己进程。

use std::process::ExitCode;

use serde_json::json;
use yanmo_host::{run_package, HostError, ModulePackage, SandboxConfig};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let payload = json!({ "ok": false, "code": error.code, "detail": error.detail });
            eprintln!("{payload}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), HostError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let module = value_of(&args, "--module")
        .ok_or_else(|| HostError::new(HostError::PACKAGE, "missing --module <dir>"))?;
    let check_only = args.iter().any(|arg| arg == "--check");

    let package = ModulePackage::open(&module)?;
    if check_only {
        println!(
            "{}",
            json!({
                "ok": true,
                "module_id": package.manifest().id,
                "version": package.manifest().version,
                "entry_bytes": package.entry_bytes(),
            })
        );
        return Ok(());
    }

    let defaults = SandboxConfig::default();
    let config = SandboxConfig {
        fuel: number_of(&args, "--fuel").unwrap_or(defaults.fuel),
        max_memory_bytes: number_of(&args, "--max-memory")
            .unwrap_or(defaults.max_memory_bytes as u64) as usize,
        max_emit_bytes: number_of(&args, "--max-emit").unwrap_or(defaults.max_emit_bytes as u64)
            as usize,
        max_total_emit_bytes: number_of(&args, "--max-total-emit")
            .unwrap_or(defaults.max_total_emit_bytes as u64) as usize,
        max_emits: number_of(&args, "--max-emits").unwrap_or(defaults.max_emits as u64) as usize,
    };
    let report = run_package(&package, config)?;
    println!(
        "{}",
        json!({
            "ok": true,
            "module_id": report.module_id,
            "status": report.status,
            "emitted": report.emitted,
            "fuel_used": report.fuel_used,
            "memory_bytes": report.memory_bytes,
        })
    );
    Ok(())
}

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn number_of(args: &[String], flag: &str) -> Option<u64> {
    value_of(args, flag).and_then(|value| value.parse().ok())
}
