//! 命令行：把一个模块目录交给沙箱跑一次，打印 JSON 报告。
//!
//! ```text
//! yanmo-host --module <目录> [--check] [--fuel N] [--max-memory BYTES]
//!            [--max-emit BYTES] [--max-total-emit BYTES] [--max-emits N]
//! ```
//!
//! `--check` 只做静态校验（清单 / 入口 / 包内文件），不执行载荷。
//! 这个二进制是**独立进程**：核心与壳将来通过协议拉起它，而不是把沙箱链进自己进程。

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use std::io::BufRead;

use serde_json::json;
use yanmo_host::{
    run_package_with_core, run_package_with_core_shared, CoreLink, HostError, ModulePackage,
    SandboxConfig,
};
use yanmo_ipc::Client;

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
    let resident = args.iter().any(|arg| arg == "--resident");

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
        max_read_arg_bytes: number_of(&args, "--max-read-arg")
            .unwrap_or(defaults.max_read_arg_bytes as u64) as usize,
        max_read_result_bytes: number_of(&args, "--max-read-result")
            .unwrap_or(defaults.max_read_result_bytes as u64) as usize,
        max_reads: number_of(&args, "--max-reads").unwrap_or(defaults.max_reads as u64) as usize,
    };
    let token_stdin = args.iter().any(|arg| arg == "--token-stdin");
    let token = if token_stdin {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|error| HostError::new(HostError::CORE, error.to_string()))?;
        Some(line.trim_end_matches(['\r', '\n']).to_string())
    } else {
        value_of(&args, "--token").map(str::to_string)
    };
    let core_text = value_of(&args, "--core").map(str::to_string);
    if core_text.is_some() != token.is_some() {
        return Err(HostError::new(
            HostError::CORE,
            "--core and --token must be given together",
        ));
    }
    let core_addr: Option<SocketAddr> = match core_text.as_deref() {
        Some(addr) => Some(addr.parse().map_err(|_| {
            HostError::new(HostError::CORE, "invalid --core address (expected ip:port)")
        })?),
        None => None,
    };
    let client = if resident {
        None
    } else {
        match (core_addr, token.as_deref()) {
            (Some(addr), Some(token)) => Some(Client::connect(addr, token).map_err(|error| {
                HostError::new(HostError::CORE, format!("{}: {}", error.code, error.detail))
            })?),
            _ => None,
        }
    };
    if resident {
        let core = match (core_addr, token) {
            (Some(addr), Some(token)) => {
                Some(Arc::new(Mutex::new(CoreLink::new(addr, token))))
            }
            _ => None,
        };
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line.map_err(|error| HostError::new(HostError::CORE, error.to_string()))?;
            match line.trim() {
                "run" => match run_package_with_core_shared(&package, config, core.clone()) {
                    Ok(report) => {
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
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    Err(error) => {
                        println!(
                            "{}",
                            json!({ "ok": false, "code": error.code, "detail": error.detail })
                        );
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                },
                "stop" => return Ok(()),
                "" => {}
                other => {
                    eprintln!(
                        "{}",
                        json!({ "ok": false, "code": "host.bad_command", "detail": other })
                    );
                }
            }
        }
        return Ok(());
    }

    let report = run_package_with_core(&package, config, client)?;
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
