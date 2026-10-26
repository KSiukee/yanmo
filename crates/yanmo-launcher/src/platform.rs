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

/// 被断链拒掉的唤起也要有一个终局条目（不许只剩沉默）。
fn note_cut(outcomes: &super::platform_outcomes::Outcomes, module_id: &str) {
    if let Ok(mut map) = outcomes.lock() {
        map.entry(module_id.to_string()).or_insert_with(|| crate::ModuleRun {
            module_id: module_id.to_string(),
            ok: false,
            code: Some("launch.chain_cut".to_string()),
            memory_bytes: 0,
            fuel_used: 0,
        });
    }
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
        // 读取闸门：未认证的第三方模块一次运行只许读一篇文章。认证模块与官方模块不走闸
        //（认证的价值就是更少摩擦），但它们的读照样进读账。
        let exempt = manifest.id.starts_with(yanmo_core::store::OFFICIAL_PREFIX)
            || store.module_card(&manifest.id).map(|card| card.certified).unwrap_or(false);
        let gate = yanmo_core::store::ModuleData::open(yanmo_core::store::ModuleData::path_beside(
            &request.db_path,
        ))
        .ok()
        .map(|data| yanmo_core::store::ReadGate::new(data, exempt));
        let mut handler =
            ModuleIpcHandler::new(store, manifest.id.clone()).with_read_log(read_log.clone());
        if let Some(gate) = gate {
            handler = handler.with_read_gate(gate);
        }
        handlers.insert(manifest.id, handler);
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
        // 宿主的每一行报告（一次运行一行 JSON、失败一行 stderr）**都接过来**：
        // 原样转发给壳（壳那边的行为不变），同时攒成"每个模块的终局"。
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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

    // 壳发来的 `run <id>`：既用来补终局，也用来给"唤起留痕"起表。
    let requested: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let starts: Arc<Mutex<HashMap<String, (i64, Instant, String)>>> = Arc::new(Mutex::new(HashMap::new()));
    // 每个模块的终局：宿主（及其子进程）把报告打在 stdout / stderr 上，
    // 一边转发给壳、一边攒成"每个模块一条"（**失败也要有名字**）。
    // 同时记「唤起留痕」：谁唤起的、跑了多久、结果如何（链路可见的地基）。
    let recorder = Arc::new(super::platform_outcomes::RunRecorder {
        starts: starts.clone(),
        data: yanmo_core::store::ModuleData::open(
            yanmo_core::store::ModuleData::path_beside(&request.db_path),
        )
        .ok()
        .map(|data| Arc::new(Mutex::new(data))),
    });
    let outcomes = super::platform_outcomes::spawn_readers(&mut child, recorder.clone());

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
    // 强杀时没跑完的那些模块得有两个字的终局；每个 `run` 的时刻给留痕算时长。
    let requested_in = requested.clone();
    let starts_in = starts.clone();
    let cut_db = request.db_path.clone();
    let outcomes_cut = outcomes.clone();
    let recorder_cut = recorder.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if let Some(rest) = line.strip_prefix("run ") {
                // `run <id>` 或 `run <id> from:<模块 id>`（后者 = 模块间唤起）
                let mut parts = rest.split_whitespace();
                let module_id = parts.next().unwrap_or_default().to_string();
                let from = parts.find_map(|part| part.strip_prefix("from:").map(str::to_string));
                if module_id.is_empty() {
                    continue;
                }
                if let Ok(mut list) = requested_in.lock() {
                    list.push(module_id.clone());
                }
                let trigger = match &from {
                    Some(from) => format!("chain:{from}"),
                    None => "user".to_string(),
                };
                if let Ok(mut map) = starts_in.lock() {
                    map.entry(module_id.clone()).or_insert_with(|| {
                        (yanmo_core::time::now_millis(), Instant::now(), trigger.clone())
                    });
                }
                // 断链：作者掐断的边**一行都不转发**，但"被拒"要留痕、要有终局。
                if let Some(from) = &from {
                    let cut = yanmo_core::store::ModuleData::open(
                        yanmo_core::store::ModuleData::path_beside(&cut_db),
                    )
                    .map(|data| data.is_cut(from, &module_id).unwrap_or(false))
                    .unwrap_or(false);
                    if cut {
                        note_cut(&outcomes_cut, &module_id);
                        recorder_cut.note_cut(&module_id, from);
                        continue;
                    }
                }
            }
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
    let mut stopped_by_shell = false;
    let status = loop {
        if shell_gone.load(Ordering::Relaxed) {
            stopped_by_shell = true;
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

    let (modules, peak_memory_bytes) = super::platform_outcomes::summarize(&outcomes);
    let requested_ids = requested.lock().map(|list| list.clone()).unwrap_or_default();
    let modules = super::platform_outcomes::finalize(modules, &requested_ids, timed_out);
    // 没等到任何报告行的那些：留痕也要有一个终局（不能只活在内存里）。
    recorder.note_unfinished(timed_out);

    Ok(LaunchReport {
        module_id: "platform".to_string(),
        // 壳关掉 stdin = **正常收摊**（不是失败）：别让"用户关窗"看起来像"宿主崩了"
        exit_code: if stopped_by_shell { Some(0) } else { status.code() },
        timed_out,
        stdout: String::new(),
        stderr: String::new(),
        ipc_error,
        job_assigned,
        modules,
        peak_memory_bytes,
    })
}

fn from_ipc(error: IpcError) -> LaunchError {
    LaunchError::new(LaunchError::IPC, format!("{}: {}", error.code, error.detail))
}
