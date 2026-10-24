//! 模块运行：**一个平台 launcher + 一个多模块宿主**。
//!
//! 壳只做三件事：核对受管包指纹、拉起/喂 `yanmo-launch --modules-root`、按模块
//! id 记录宿主逐行交回的报告。停止是**停整个平台**（WASM 没有安全的单线程终止）。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::Store;

use super::module_run_types::{ModuleRunStarted, ModuleStatus};

struct Platform {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    running: bool,
    statuses: HashMap<String, ModuleStatus>,
    error: String,
    last_activity: Instant,
}

impl Default for Platform {
    fn default() -> Self {
        Self {
            child: None,
            stdin: None,
            running: false,
            statuses: HashMap::new(),
            error: String::new(),
            last_activity: Instant::now(),
        }
    }
}

/// 平台运行登记（全局一个；多模块状态按 id 分开）。
#[derive(Default)]
pub struct ModuleRuns {
    platform: Arc<Mutex<Platform>>,
}

impl ModuleRuns {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Platform>, ApiError> {
        self.platform
            .lock()
            .map_err(|_| ApiError::new("shell.module_launch_failed").caused_by("platform lock poisoned"))
    }

    fn ensure_platform(&self, data: &AppData) -> Result<(), ApiError> {
        {
            let platform = self.lock()?;
            if platform.running {
                return Ok(());
            }
        }
        let host = resolve_bin("yanmo-host", "YANMO_HOST_BIN")?;
        let launcher = resolve_bin("yanmo-launch", "YANMO_LAUNCH_BIN")?;
        let modules = data.data_dir().join("modules");
        let mut command = Command::new(&launcher);
        command
            .arg("--db")
            .arg(data.db_path())
            .arg("--modules-root")
            .arg(&modules)
            .arg("--host")
            .arg(&host)
            .arg("--timeout-secs")
            .arg("3600")
            .arg("--idle-secs")
            .arg("300")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|error| {
            ApiError::new("shell.module_launch_failed").caused_by(error)
        })?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ApiError::new("shell.module_launch_failed")
                .caused_by("platform stdin was not piped"));
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        {
            let mut platform = self.lock()?;
            platform.child = Some(child);
            platform.stdin = Some(stdin);
            platform.running = true;
            platform.statuses.clear();
            platform.error.clear();
            platform.last_activity = Instant::now();
        }
        let shared = self.platform.clone();
        if let Some(stdout) = stdout {
            thread::spawn(move || pump_stdout(stdout, shared));
        } else {
            return Err(ApiError::new("shell.module_launch_failed").caused_by("platform stdout missing"));
        }
        if let Some(stderr) = stderr {
            let shared = self.platform.clone();
            thread::spawn(move || pump_stderr(stderr, shared));
        }
        monitor(self.platform.clone());
        Ok(())
    }

    fn send(&self, command: &str) -> Result<(), ApiError> {
        let mut platform = self.lock()?;
        let Some(stdin) = platform.stdin.as_mut() else {
            return Err(ApiError::new("shell.module_not_running"));
        };
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|error| ApiError::new("shell.module_launch_failed").caused_by(error))?;
        platform.last_activity = Instant::now();
        Ok(())
    }

    pub fn stop_platform(&self) -> Result<(), ApiError> {
        let mut platform = self.lock()?;
        if !platform.running {
            return Ok(());
        }
        if let Some(stdin) = platform.stdin.as_mut() {
            let _ = stdin.write_all(b"stop\n").and_then(|_| stdin.flush());
        }
        for status in platform.statuses.values_mut() {
            status.running = false;
            status.stopped = true;
        }
        drop(platform);
        let shared = self.platform.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(5));
            if let Ok(mut platform) = shared.lock() {
                if platform.running {
                    if let Some(child) = platform.child.as_mut() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    platform.child = None;
                    platform.stdin = None;
                    platform.running = false;
                    for status in platform.statuses.values_mut() {
                        status.running = false;
                        status.stopped = true;
                    }
                }
            }
        });
        Ok(())
    }

    fn kill_platform(&self) {
        if let Ok(mut platform) = self.platform.lock() {
            if let Some(child) = platform.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            platform.child = None;
            platform.stdin = None;
            platform.running = false;
            for status in platform.statuses.values_mut() {
                status.running = false;
                status.stopped = true;
            }
        }
    }
}

impl Drop for ModuleRuns {
    fn drop(&mut self) {
        self.kill_platform();
    }
}

fn pump_stdout(reader: impl Read + Send + 'static, platform: Arc<Mutex<Platform>>) {
    let reader = BufReader::new(reader);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(module_id) = value.get("module_id").and_then(|value| value.as_str()) else {
            continue;
        };
        let Ok(mut platform) = platform.lock() else { break };
        let status = platform
            .statuses
            .entry(module_id.to_string())
            .or_insert_with(ModuleStatus::unknown);
        status.known = true;
        status.running = false;
        status.timed_out = value
            .get("timed_out")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        status.emitted = value
            .get("emitted")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        status.stderr = value
            .get("stderr")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if value.get("ok").and_then(|value| value.as_bool()) == Some(false) {
            let code = value
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("module.run_failed");
            let detail = value
                .get("detail")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            status.error = Some(format!("{code}: {detail}"));
        }
        platform.last_activity = Instant::now();
    }
}

fn pump_stderr(reader: impl Read + Send + 'static, platform: Arc<Mutex<Platform>>) {
    let reader = BufReader::new(reader);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if let Ok(mut platform) = platform.lock() {
            platform.error.push_str(&line);
            platform.error.push('\n');
        }
    }
}

fn monitor(platform: Arc<Mutex<Platform>>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(200));
        let Ok(mut platform) = platform.lock() else {
            return;
        };
        if !platform.running {
            return;
        }
        let exited = match platform.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(_) => true,
            },
            None => true,
        };
        if exited {
            platform.child = None;
            platform.stdin = None;
            platform.running = false;
            for status in platform.statuses.values_mut() {
                status.running = false;
            }
            return;
        }
    });
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_run(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<ModuleRunStarted, ApiError> {
    let record = data.with_store(|store: &mut Store| store.module_record(&module_id))?;
    if !record.is_active() {
        return Err(ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::MODULE_DISABLED,
            [("module_id", module_id.clone())],
        )));
    }
    let managed = data.data_dir().join("modules").join(&module_id);
    data.with_store(|store: &mut Store| store.verify_module_package(&module_id, &managed))?;

    runs.ensure_platform(&data)?;
    runs.send(&format!("run {module_id}"))?;
    let mut platform = runs.lock()?;
    let status = platform
        .statuses
        .entry(module_id.clone())
        .or_insert_with(ModuleStatus::unknown);
    status.known = true;
    status.running = true;
    status.stopped = false;
    status.error = None;
    status.emitted.clear();
    platform.last_activity = Instant::now();
    Ok(ModuleRunStarted { module_id })
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_status(
    runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<ModuleStatus, ApiError> {
    let platform = runs.lock()?;
    let mut status = platform
        .statuses
        .get(&module_id)
        .cloned()
        .unwrap_or_else(ModuleStatus::unknown);
    if status.known && status.running && platform.running {
        status.idle_seconds = platform.last_activity.elapsed().as_secs();
    } else {
        status.idle_seconds = 0;
    }
    if !platform.running {
        status.running = false;
    }
    Ok(status)
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_stop(runs: State<'_, ModuleRuns>, _module_id: String) -> Result<(), ApiError> {
    // 平台里只有一个宿主；停一个就是停整个平台。
    runs.stop_platform()
}

fn resolve_bin(name: &str, env_name: &str) -> Result<PathBuf, ApiError> {
    if let Ok(path) = std::env::var(env_name) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()
        .map_err(|_| ApiError::new("shell.module_platform_unavailable"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| ApiError::new("shell.module_platform_unavailable"))?;
    let candidates = if cfg!(windows) {
        vec![dir.join(format!("{name}.exe")), dir.join(name)]
    } else {
        vec![dir.join(name)]
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| ApiError::new("shell.module_platform_unavailable"))
}
