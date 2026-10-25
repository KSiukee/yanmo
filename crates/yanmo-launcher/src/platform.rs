//! 平台模式：**一个 launcher 服务多个模块**（多模块宿主）。
//!
//! 与单模块启动的区别：
//! - 宿主收 `--modules-root`，按 `run <id>` 惰性加载；
//! - 核心服务端是 `serve_multi`，每个模块一个 `ModuleIpcHandler`，按请求里的
//!   `module_id` 路由；
//! - 宿主 stdout/stderr 继承给壳，每次运行一行 JSON 报告。

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use yanmo_core::store::{new_ipc_token, ModuleIpcHandler, Store};
use yanmo_ipc::{bind_loopback, serve_multi};
use yanmo_proto::ipc::IpcError;
use yanmo_proto::manifest::ModuleManifest;

use crate::job::JobGuard;
use crate::{LaunchError, LaunchReport, JOB_MEMORY_LIMIT};

#[derive(Debug, Clone)]
pub struct LaunchPlatformRequest {
    pub db_path: PathBuf,
    pub modules_root: PathBuf,
    pub host_bin: PathBuf,
    pub timeout: Duration,
    pub idle_secs: u64,
}

pub fn launch_platform(request: &LaunchPlatformRequest) -> Result<LaunchReport, LaunchError> {
    if request.timeout.is_zero() {
        return Err(LaunchError::new(LaunchError::TIMEOUT, "timeout must be > 0"));
    }
    if !request.host_bin.is_file() {
        return Err(LaunchError::new(
            LaunchError::SPAWN,
            format!("host binary not found: {}", request.host_bin.display()),
        ));
    }

    let mut handlers: HashMap<String, ModuleIpcHandler> = HashMap::new();
    // 读账落在库旁边（跟库走）：每次读一行，模块读不到它、也不影响读本身。
    let read_log = yanmo_core::store::ReadLog::new(
        request
            .db_path
            .parent()
            .map(|dir| dir.join(yanmo_core::store::READ_LOG_FILE))
            .unwrap_or_else(|| PathBuf::from(yanmo_core::store::READ_LOG_FILE)),
    );
    let mut network_modules: Vec<String> = Vec::new();
    let mut own_data_modules: Vec<String> = Vec::new();
    let mut credentials: HashMap<String, Vec<yanmo_core::store::CredentialBinding>> =
        HashMap::new();
    let mut allowlist: Option<Vec<String>> = None;
    for entry in fs::read_dir(&request.modules_root).map_err(|error| {
        LaunchError::new(LaunchError::MANIFEST, format!("{}: {error}", request.modules_root.display()))
    })? {
        let entry = entry.map_err(|error| LaunchError::new(LaunchError::MANIFEST, error.to_string()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        if !manifest_path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&manifest_path)
            .map_err(|error| LaunchError::new(LaunchError::MANIFEST, error.to_string()))?;
        let manifest = ModuleManifest::from_json(&text)
            .map_err(|error| LaunchError::new(LaunchError::MANIFEST, error.to_string()))?;
        let store = Store::open(&request.db_path)
            .map_err(|error| LaunchError::new(LaunchError::DB, error.to_string()))?;
        let offline = store.network_offline().unwrap_or(false);
        if allowlist.is_none() {
            allowlist = Some(store.network_allowlist().unwrap_or_default());
        }
        if !offline
            && store
                .module_card(&manifest.id)
                .map(|card| card.network)
                .unwrap_or(false)
        {
            network_modules.push(manifest.id.clone());
        }
        if store
            .module_card(&manifest.id)
            .map(|card| card.own_data)
            .unwrap_or(false)
        {
            own_data_modules.push(manifest.id.clone());
        }
        let bindings = store.module_credentials(&manifest.id).unwrap_or_default();
        if !bindings.is_empty() {
            credentials.insert(manifest.id.clone(), bindings);
        }
        handlers.insert(
            manifest.id.clone(),
            ModuleIpcHandler::new(store, manifest.id).with_read_log(read_log.clone()),
        );
    }
    if handlers.is_empty() {
        return Err(LaunchError::new(
            LaunchError::MANIFEST,
            format!("no modules under {}", request.modules_root.display()),
        ));
    }

    let listener = bind_loopback().map_err(from_ipc)?;
    let addr = listener
        .local_addr()
        .map_err(|error| LaunchError::new(LaunchError::IPC, error.to_string()))?;
    let token = new_ipc_token()
        .map_err(|error| LaunchError::new(LaunchError::IPC, error.to_string()))?;

    let stop_server = Arc::new(AtomicBool::new(false));
    let server_stop = stop_server.clone();
    let server_token = token.clone();
    let handlers = Arc::new(Mutex::new(handlers));
    let server = std::thread::spawn(move || {
        serve_multi(
            listener,
            server_token,
            handlers,
            server_stop,
            Duration::from_millis(250),
        )
    });

    let mut command = Command::new(&request.host_bin);
    command
        .arg("--modules-root")
        .arg(&request.modules_root)
        .arg("--core")
        .arg(addr.to_string())
        .arg("--token-stdin")
        .arg("--resident")
        .arg("--idle-secs")
        .arg(request.idle_secs.max(1).min(3600).to_string());
    if !network_modules.is_empty() {
        command.arg("--network-modules").arg(network_modules.join(","));
    }
    if !own_data_modules.is_empty() {
        command
            .arg("--own-data-modules")
            .arg(own_data_modules.join(","));
    }
    command
        .arg("--modules-db")
        .arg(request.db_path.with_file_name("modules.db"));
    if !credentials.is_empty() {
        command
            .arg("--credentials-json")
            .arg(serde_json::to_string(&credentials).unwrap_or_else(|_| "{}".to_string()));
    }
    if let Some(hosts) = allowlist.as_ref().filter(|hosts| !hosts.is_empty()) {
        command.arg("--network-allowlist").arg(hosts.join(","));
    }
    if let Some(parent) = request.db_path.parent() {
        command
            .arg("--network-log")
            .arg(parent.join("network-audit.jsonl"));
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            stop_server.store(true, Ordering::Relaxed);
            let _ = server.join();
            return Err(LaunchError::new(LaunchError::SPAWN, error.to_string()));
        }
    };
    let mut job = JobGuard::create_with_limit(JOB_MEMORY_LIMIT, 64);
    let job_assigned = job.as_mut().map(|guard| guard.assign(&child)).unwrap_or(false);

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        stop_server.store(true, Ordering::Relaxed);
        let _ = server.join();
        return Err(LaunchError::new(LaunchError::SPAWN, "host stdin was not piped"));
    };
    if let Err(error) = stdin.write_all(format!("{token}\n").as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        stop_server.store(true, Ordering::Relaxed);
        let _ = server.join();
        return Err(LaunchError::new(LaunchError::SPAWN, error.to_string()));
    }
    let shell_gone = Arc::new(AtomicBool::new(false));
    let shell_gone_watch = shell_gone.clone();
    let host_stdin = Arc::new(Mutex::new(stdin));
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let Ok(mut guard) = host_stdin.lock() else { break };
            if guard.write_all(format!("{line}\n").as_bytes()).is_err() {
                break;
            }
            let _ = guard.flush();
        }
        // 壳的 stdin 关了（正常退出或强杀）：平台不再有主人，收掉宿主。
        shell_gone_watch.store(true, Ordering::Relaxed);
    });

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if shell_gone.load(Ordering::Relaxed) {
            if let Some(guard) = &job {
                guard.terminate();
            }
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| LaunchError::new(LaunchError::WAIT, error.to_string()))?;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                stop_server.store(true, Ordering::Relaxed);
                let _ = server.join();
                return Err(LaunchError::new(LaunchError::WAIT, error.to_string()));
            }
        }
        if started.elapsed() >= request.timeout {
            timed_out = true;
            if let Some(guard) = &job {
                guard.terminate();
            }
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| LaunchError::new(LaunchError::WAIT, error.to_string()))?;
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    stop_server.store(true, Ordering::Relaxed);
    let ipc_error = match server.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(format!("{}: {}", error.code, error.detail)),
        Err(_) => Some("ipc.server_panicked".to_string()),
    };

    Ok(LaunchReport {
        module_id: "platform".to_string(),
        exit_code: status.code(),
        timed_out,
        stdout: String::new(),
        stderr: String::new(),
        ipc_error,
        job_assigned,
    })
}

fn from_ipc(error: IpcError) -> LaunchError {
    LaunchError::new(LaunchError::IPC, format!("{}: {}", error.code, error.detail))
}
