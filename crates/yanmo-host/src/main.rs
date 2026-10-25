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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use std::io::BufRead;

use serde_json::json;
use yanmo_host::{
    run_package_with_credentials, run_package_with_core, run_package_with_core_shared, CoreLink, HostError,
    ModuleData, ModulePackage, SandboxConfig,
};
use yanmo_ipc::Client;

mod args;
mod credential_args;
mod supervisor;

use crate::args::{number_of, value_of};

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
    let module_id_arg = value_of(&args, "--module-id").map(str::to_string);
    let check_only = args.iter().any(|arg| arg == "--check");
    let resident = args.iter().any(|arg| arg == "--resident");
    let modules_root = value_of(&args, "--modules-root").map(PathBuf::from);
    let idle_secs = number_of(&args, "--idle-secs").unwrap_or(300).clamp(1, 3600);
    let credentials_json = value_of(&args, "--credentials-json").map(str::to_string);
    let network_log = value_of(&args, "--network-log").map(PathBuf::from);
    let modules_db = value_of(&args, "--modules-db").map(PathBuf::from);
    let own_data_modules: Vec<String> = value_of(&args, "--own-data-modules")
        .map(|value| value.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    let network_allowlist: Vec<String> = value_of(&args, "--network-allowlist")
        .map(|value| value.split(',').filter(|host| !host.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    let network_modules: Vec<String> = value_of(&args, "--network-modules")
        .map(|value| value.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
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
        allow_network: args.iter().any(|arg| arg == "--allow-network"),
        allow_own_data: args.iter().any(|arg| arg == "--allow-own-data"),
        max_network_requests: number_of(&args, "--max-network-requests")
            .unwrap_or(defaults.max_network_requests as u64) as usize,
        max_network_bytes: number_of(&args, "--max-network-bytes")
            .unwrap_or(defaults.max_network_bytes as u64) as usize,
        max_total_network_bytes: number_of(&args, "--max-total-network-bytes")
            .unwrap_or(defaults.max_total_network_bytes as u64) as usize,
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
            (Some(addr), Some(token)) => Some(Client::connect_module(addr, token, module_id_arg.as_deref()).map_err(|error| {
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
        let exe = std::env::current_exe()
            .map_err(|error| HostError::new(HostError::CORE, error.to_string()))?;
        let mut children: HashMap<String, std::process::Child> = HashMap::new();
        let mut crashes: HashMap<String, Vec<Instant>> = HashMap::new();
        let mut last_activity = Instant::now();
        loop {
            for (crashed_id, code) in supervisor::reap(&mut children) {
                if code == 0 {
                    crashes.remove(&crashed_id);
                    continue;
                }
                let now = Instant::now();
                let history = crashes.entry(crashed_id.clone()).or_default();
                history.retain(|at| now.duration_since(*at) < Duration::from_secs(600));
                history.push(now);
                if history.len() <= 2 {
                    match supervisor::spawn_child(
                        &exe,
                        &root,
                        &crashed_id,
                        core_addr,
                        token.as_deref(),
                        &network_modules,
                        credentials_json.as_deref(),
                        network_log.as_deref(),
                        &network_allowlist,
                        modules_db.as_deref(),
                        own_data_modules.iter().any(|id| id == &crashed_id),
                    ) {
                        Ok(child) => {
                            children.insert(crashed_id.clone(), child);
                        }
                        Err(error) => {
                            println!(
                                "{}",
                                json!({ "ok": false, "module_id": crashed_id, "code": "host.crash_restart_failed", "detail": error.to_string() })
                            );
                        }
                    }
                } else {
                    println!(
                        "{}",
                        json!({ "ok": false, "module_id": crashed_id, "code": "host.crash_budget", "detail": format!("host exited with {code}; too many crashes") })
                    );
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
            let line = match receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(Ok(line)) => {
                    last_activity = Instant::now();
                    line
                }
                Ok(Err(error)) => {
                    kill_all(&mut children);
                    return Err(HostError::new(HostError::CORE, error.to_string()));
                }
                Err(_) => {
                    if children.is_empty()
                        && last_activity.elapsed() >= Duration::from_secs(idle_secs)
                    {
                        return Ok(());
                    }
                    continue;
                }
            };
            let command = line.trim();
            if command == "stop" {
                kill_all(&mut children);
                return Ok(());
            }
            if let Some(module_id) = command.strip_prefix("stop ").map(str::trim) {
                if let Some(mut child) = children.remove(module_id) {
                    let _ = child.kill();
                    let _ = child.wait();
                    println!(
                        "{}",
                        json!({
                            "ok": false,
                            "module_id": module_id,
                            "code": HostError::STOPPED,
                            "detail": "stopped by user",
                        })
                    );
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
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
            crashes.remove(module_id);
            if children.contains_key(module_id) {
                eprintln!(
                    "{}",
                    json!({ "ok": false, "module_id": module_id, "code": "host.already_running" })
                );
                continue;
            }
            match supervisor::spawn_child(
                &exe,
                &root,
                module_id,
                core_addr,
                token.as_deref(),
                &network_modules,
                credentials_json.as_deref(),
                network_log.as_deref(),
                &network_allowlist,
                modules_db.as_deref(),
                own_data_modules.iter().any(|id| id == module_id),
            ) {
                Ok(child) => {
                    children.insert(module_id.to_string(), child);
                }
                Err(error) => {
                    eprintln!(
                        "{}",
                        json!({ "ok": false, "module_id": module_id, "code": "host.spawn_failed", "detail": error.to_string() })
                    );
                }
            }
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

    let credentials = credential_args::credentials_for(module_id_arg.as_deref(), credentials_json.as_deref());
    let data = match value_of(&args, "--modules-db") {
        Some(path) if config.allow_own_data => Some(Arc::new(Mutex::new(ModuleData::open(path)?))),
        _ => None,
    };
    let report = if data.is_none() && credentials.is_empty() && network_log.is_none() {
        run_package_with_core(&package, config, client)?
    } else {
        run_package_with_credentials(
            &package,
            config,
            client,
            credentials,
            network_log.clone(),
            network_allowlist.clone(),
            data,
        )?
    };
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



fn kill_all(children: &mut HashMap<String, std::process::Child>) {
    for (_, mut child) in children.drain() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
