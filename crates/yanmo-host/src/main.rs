//! 命令行：把一个模块目录交给沙箱跑一次，打印 JSON 报告。
//!
//! ```text
//! yanmo-host --module <目录> [--check] [--fuel N] [--max-memory BYTES]
//!            [--max-emit BYTES] [--max-total-emit BYTES] [--max-emits N]
//! ```
//!
//! `--check` 只做静态校验（清单 / 入口 / 包内文件），不执行载荷。
//! 这个二进制是**独立进程**：核心与壳将来通过协议拉起它，而不是把沙箱链进自己进程。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use std::io::BufRead;

use serde_json::json;
use yanmo_host::{
    run_package_with_core, run_package_with_core_shared, run_package_with_core_shared_cancellable, CoreLink, HostError,
    ModulePackage, SandboxConfig,
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
    let module = value_of(&args, "--module");
    let check_only = args.iter().any(|arg| arg == "--check");
    let resident = args.iter().any(|arg| arg == "--resident");
    let modules_root = value_of(&args, "--modules-root").map(PathBuf::from);
    let idle_secs = number_of(&args, "--idle-secs").unwrap_or(300).clamp(1, 3600);
    if module.is_none() && modules_root.is_none() {
        return Err(HostError::new(
            HostError::PACKAGE,
            "missing --module <dir> or --modules-root <dir>",
        ));
    }

    let package = module.map(ModulePackage::open).transpose()?;
    if check_only {
        let package = package
            .as_ref()
            .ok_or_else(|| HostError::new(HostError::PACKAGE, "--check needs --module <dir>"))?;
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
    if resident && modules_root.is_some() {
        let root = modules_root.expect("checked");
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let packages: Arc<Mutex<HashMap<String, Arc<ModulePackage>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let links: Arc<Mutex<HashMap<String, Arc<Mutex<CoreLink>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let running: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let output = Arc::new(Mutex::new(()));
        loop {
            let line = match receiver.recv_timeout(std::time::Duration::from_secs(idle_secs)) {
                Ok(Ok(line)) => line,
                Ok(Err(error)) => {
                    return Err(HostError::new(HostError::CORE, error.to_string()));
                }
                Err(_) => {
                    if active.load(Ordering::Relaxed) == 0 {
                        return Ok(());
                    }
                    continue;
                }
            };
            let command = line.trim();
            if command == "stop" {
                if let Ok(map) = running.lock() {
                    for cancel in map.values() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
                return Ok(());
            }
            if let Some(module_id) = command.strip_prefix("stop ").map(str::trim) {
                if let Ok(map) = running.lock() {
                    if let Some(cancel) = map.get(module_id) {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
                continue;
            }
            let Some(module_id) = command.strip_prefix("run ").map(str::trim) else {
                eprintln!(
                    "{}",
                    json!({ "ok": false, "code": "host.bad_command", "detail": command })
                );
                continue;
            };
            if module_id.is_empty() {
                eprintln!(
                    "{}",
                    json!({ "ok": false, "code": "host.bad_command", "detail": "missing module id" })
                );
                continue;
            }
            {
                let map = running.lock().map_err(|_| {
                    HostError::new(HostError::CORE, "running map poisoned")
                })?;
                if map.contains_key(module_id) {
                    eprintln!(
                        "{}",
                        json!({ "ok": false, "module_id": module_id, "code": "host.already_running" })
                    );
                    continue;
                }
            }
            let package = {
                let mut map = packages.lock().map_err(|_| {
                    HostError::new(HostError::CORE, "package map poisoned")
                })?;
                if !map.contains_key(module_id) {
                    match ModulePackage::open(root.join(module_id)) {
                        Ok(package) if package.manifest().id == module_id => {
                            map.insert(module_id.to_string(), Arc::new(package));
                        }
                        Ok(package) => {
                            eprintln!(
                                "{}",
                                json!({
                                    "ok": false,
                                    "code": "host.module_mismatch",
                                    "detail": format!("{} != {}", package.manifest().id, module_id)
                                })
                            );
                            continue;
                        }
                        Err(error) => {
                            eprintln!(
                                "{}",
                                json!({ "ok": false, "code": error.code, "detail": error.detail })
                            );
                            continue;
                        }
                    }
                }
                map.get(module_id).expect("inserted above").clone()
            };
            let link = match (core_addr, token.as_ref()) {
                (Some(addr), Some(token)) => {
                    let mut map = links.lock().map_err(|_| {
                        HostError::new(HostError::CORE, "link map poisoned")
                    })?;
                    Some(
                        map.entry(module_id.to_string())
                            .or_insert_with(|| {
                                Arc::new(Mutex::new(CoreLink::new_module(
                                    addr,
                                    token.clone(),
                                    module_id.to_string(),
                                )))
                            })
                            .clone(),
                    )
                }
                _ => None,
            };
            let cancel = Arc::new(AtomicBool::new(false));
            if let Ok(mut map) = running.lock() {
                map.insert(module_id.to_string(), cancel.clone());
            }
            active.fetch_add(1, Ordering::Relaxed);
            let running_workers = running.clone();
            let active_workers = active.clone();
            let output_workers = output.clone();
            let module = module_id.to_string();
            std::thread::spawn(move || {
                let result = run_package_with_core_shared_cancellable(
                    &package,
                    config,
                    link,
                    Some(cancel.clone()),
                );
                let stopped = cancel.load(Ordering::Relaxed);
                let payload = if stopped {
                    json!({
                        "ok": false,
                        "module_id": module,
                        "code": HostError::STOPPED,
                        "detail": "module was stopped",
                    })
                } else {
                    match result {
                        Ok(report) => json!({
                            "ok": true,
                            "module_id": report.module_id,
                            "status": report.status,
                            "emitted": report.emitted,
                            "fuel_used": report.fuel_used,
                            "memory_bytes": report.memory_bytes,
                        }),
                        Err(error) => json!({
                            "ok": false,
                            "module_id": module,
                            "code": error.code,
                            "detail": error.detail,
                        }),
                    }
                };
                let _guard = output_workers.lock();
                println!("{payload}");
                use std::io::Write;
                let _ = std::io::stdout().flush();
                active_workers.fetch_sub(1, Ordering::Relaxed);
                if let Ok(mut map) = running_workers.lock() {
                    map.remove(&module);
                }
            });
        }
    }

    let package = package
        .ok_or_else(|| HostError::new(HostError::PACKAGE, "--module <dir> required"))?;

    if resident {
        let core = match (core_addr, token) {
            (Some(addr), Some(token)) => {
                Some(Arc::new(Mutex::new(CoreLink::new(addr, token))))
            }
            _ => None,
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        loop {
            let line = match receiver.recv_timeout(std::time::Duration::from_secs(idle_secs)) {
                Ok(Ok(line)) => line,
                Ok(Err(error)) => {
                    return Err(HostError::new(HostError::CORE, error.to_string()));
                }
                Err(_) => return Ok(()),
            };
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
