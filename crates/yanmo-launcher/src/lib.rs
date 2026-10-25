//! 研墨模块启动器：**起只读 IPC 服务 → 拉独立宿主 → 等它跑完**。
//!
//! # 为什么单独成 crate
//!
//! 它需要回环 IPC（`yanmo-ipc`）与进程/作业对象（Windows）。默认的壳与核心**不依赖**
//! 它——模块平台只在需要时编进来，核心/壳/协议层的"零出网"边界不被破。
//!
//! # 一次启动做什么
//!
//! 1. 读模块清单，拿模块 id；
//! 2. 用数据库路径开一个独立 `Store` 连接，起 `ModuleIpcHandler` 的回环服务；
//! 3. 生成一次性 token，**从 stdin 交给宿主**（不走 argv，进程列表看不到）；
//! 4. 拉 `yanmo-host --module <dir> --core <addr> --token-stdin`；Windows 上放进
//!    作业对象（单进程 + 内存上限 + 关闭即杀）；
//! 5. 等宿主退出（有总超时）；超时就杀作业/子进程；
//! 6. 收 stdout/stderr、关服务、返回报告。
//!
//! 子 WebView 的 UI 还不在这里；这一层只负责"把沙箱进程安全地跑起来"。

#![allow(unsafe_code)] // 仅 `job` 模块的 Win32 调用；没有别的 unsafe。

mod job;
mod platform;

use std::fs;
use std::io::{BufRead, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use yanmo_core::store::{new_ipc_token, ModuleIpcHandler, Store};
use yanmo_ipc::{bind_loopback, serve_one, serve_repeated};
use yanmo_proto::ipc::IpcError;
use yanmo_proto::manifest::ModuleManifest;

use job::JobGuard;
pub use platform::{launch_platform, LaunchPlatformRequest};

/// 宿主的作业对象内存上限（wasmi + 模块；够用且不至于拖垮机器）。
pub const JOB_MEMORY_LIMIT: usize = 512 * 1024 * 1024;

/// 一次启动的输入。
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub db_path: PathBuf,
    pub module_dir: PathBuf,
    pub host_bin: PathBuf,
    pub timeout: Duration,
    /// 常驻模式：宿主等 `run`/`stop` 命令，核心连接跨运行复用。
    pub resident: bool,
    /// 常驻宿主空闲多少秒后自己退出（默认 300；0 按 300 算）。
    pub idle_secs: u64,
}

/// 一次启动的结果（给壳/管理页）。
#[derive(Debug, Clone, Serialize)]
pub struct LaunchReport {
    pub module_id: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    /// 服务端自己出的错（握手失败 / 没宿主连上 / 读请求出错）；宿主正常跑完为 None
    pub ipc_error: Option<String>,
    /// Windows 作业对象是否真的挂上了（挂不上也不算启动失败，只是没有配额）
    pub job_assigned: bool,
}

/// 启动失败的稳定码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchError {
    pub code: &'static str,
    pub detail: String,
}

impl LaunchError {
    pub const MANIFEST: &'static str = "launch.manifest";
    pub const DB: &'static str = "launch.db";
    pub const IPC: &'static str = "launch.ipc";
    pub const SPAWN: &'static str = "launch.spawn";
    pub const WAIT: &'static str = "launch.wait";
    pub const TIMEOUT: &'static str = "launch.timeout";

    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for LaunchError {}

/// 跑一次模块。
pub fn launch(request: &LaunchRequest) -> Result<LaunchReport, LaunchError> {
    if request.timeout.is_zero() {
        return Err(LaunchError::new(LaunchError::TIMEOUT, "timeout must be > 0"));
    }

    let manifest_path = request.module_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        LaunchError::new(LaunchError::MANIFEST, format!("{}: {error}", manifest_path.display()))
    })?;
    let manifest = ModuleManifest::from_json(&manifest_text)
        .map_err(|error| LaunchError::new(LaunchError::MANIFEST, error.to_string()))?;
    let module_id = manifest.id.clone();
    if !request.host_bin.is_file() {
        return Err(LaunchError::new(
            LaunchError::SPAWN,
            format!("host binary not found: {}", request.host_bin.display()),
        ));
    }

    // 先用一个短连接确认库能打开（真正服务用服务线程自己的连接）。
    {
        Store::open(&request.db_path)
            .map_err(|error| LaunchError::new(LaunchError::DB, error.to_string()))?;
    }

    let listener = bind_loopback().map_err(from_ipc)?;
    let addr = listener.local_addr().map_err(|error| {
        LaunchError::new(LaunchError::IPC, error.to_string())
    })?;
    let token = new_ipc_token().map_err(|error| {
        LaunchError::new(LaunchError::IPC, error.to_string())
    })?;

    let db_path = request.db_path.clone();
    let server_module = module_id.clone();
    let server_token = token.clone();
    let read_log = yanmo_core::store::ReadLog::new(
        request
            .db_path
            .parent()
            .map(|dir| dir.join(yanmo_core::store::READ_LOG_FILE))
            .unwrap_or_else(|| std::path::PathBuf::from(yanmo_core::store::READ_LOG_FILE)),
    );
    let resident = request.resident;
    let stop_server = Arc::new(AtomicBool::new(false));
    let server_stop = stop_server.clone();
    let server = std::thread::spawn(move || {
        let store = Store::open(&db_path)
            .map_err(|error| IpcError::new(error.code(), error.to_string()))?;
        let mut handler =
            ModuleIpcHandler::new(store, server_module).with_read_log(read_log);
        if resident {
            serve_repeated(
                listener,
                server_token,
                &mut handler,
                server_stop,
                Duration::from_millis(250),
            )
        } else {
            serve_one(listener, server_token, &mut handler)
        }
    });

    let mut command = Command::new(&request.host_bin);
    command
        .arg("--module")
        .arg(&request.module_dir)
        .arg("--core")
        .arg(addr.to_string())
        .arg("--token-stdin")
        .arg("--idle-secs")
        .arg(request.idle_secs.max(1).min(3600).to_string())
        .stdin(Stdio::piped());
    if request.resident {
        command.arg("--resident");
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = server.join();
            return Err(LaunchError::new(LaunchError::SPAWN, error.to_string()));
        }
    };

    let mut job = JobGuard::create(JOB_MEMORY_LIMIT);
    let job_assigned = job.as_mut().map(|guard| guard.assign(&child)).unwrap_or(false);

    // 宿主在读到 token 之前不会连；先挂作业对象再交 token，缩小无配额窗口。
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = server.join();
        return Err(LaunchError::new(LaunchError::SPAWN, "host stdin was not piped"));
    };
    if let Err(error) = stdin.write_all(format!("{token}\n").as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = server.join();
        return Err(LaunchError::new(LaunchError::SPAWN, error.to_string()));
    }
    if request.resident {
        // 常驻：宿主 stdin 保持打开，把启动器收到的控制行转给宿主。
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
        });
    } else {
        drop(stdin);
    }

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
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

    let mut stdout = String::new();
    let mut stderr = String::new();
    if !request.resident {
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut stdout);
        }
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
    }

    stop_server.store(true, Ordering::Relaxed);
    let ipc_error = match server.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(format!("{}: {}", error.code, error.detail)),
        Err(_) => Some("ipc.server_panicked".to_string()),
    };

    Ok(LaunchReport {
        module_id,
        exit_code: status.code(),
        timed_out,
        stdout,
        stderr,
        ipc_error,
        job_assigned,
    })
}

fn from_ipc(error: IpcError) -> LaunchError {
    LaunchError::new(LaunchError::IPC, format!("{}: {}", error.code, error.detail))
}
