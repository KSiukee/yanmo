//! 模块运行：**后台拉起独立启动器**，管理页能看状态、能停。
//!
//! 壳不链接沙箱/IPC；这里只做三件事：核对受管包指纹、拉起 `yanmo-launch`、记录子进程。
//! 停止 = 杀掉启动器进程；启动器的作业对象是 kill-on-close，宿主会跟着退。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use super::module_run_types::{ModuleRunStarted, ModuleStatus};
use yanmo_core::store::Store;

/// 一次运行在壳里的登记（子进程 + 已收到的输出）。
struct RunSlot {
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    output: String,
    error: String,
    running: bool,
    stopped: bool,
    exit_code: Option<i32>,
    timed_out: bool,
    last_activity: std::time::Instant,
}

impl Default for RunSlot {
    fn default() -> Self {
        Self {
            child: None,
            stdin: None,
            output: String::new(),
            error: String::new(),
            running: false,
            stopped: false,
            exit_code: None,
            timed_out: false,
            last_activity: std::time::Instant::now(),
        }
    }
}

/// 运行登记表：按模块 id 保存**当前这一次**运行；同一模块同时只跑一个。
#[derive(Default)]
pub struct ModuleRuns {
    runs: Arc<Mutex<HashMap<String, Arc<Mutex<RunSlot>>>>>,
}

impl ModuleRuns {
    fn slot(&self, module_id: &str) -> Option<Arc<Mutex<RunSlot>>> {
        self.runs.lock().ok()?.get(module_id).cloned()
    }

    fn is_running(&self, module_id: &str) -> bool {
        self.slot(module_id)
            .and_then(|slot| slot.lock().ok().map(|guard| guard.running))
            .unwrap_or(false)
    }

    fn start(&self, module_id: &str, mut child: Child) -> Result<(), ApiError> {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdin = child.stdin.take();
        let slot = Arc::new(Mutex::new(RunSlot {
            child: Some(child),
            stdin,
            last_activity: std::time::Instant::now(),
            running: true,
            ..RunSlot::default()
        }));
        if let Ok(mut runs) = self.runs.lock() {
            runs.insert(module_id.to_string(), slot.clone());
        } else {
            return Err(ApiError::new("shell.module_launch_failed").caused_by("run registry lock poisoned"));
        }
        if let Some(reader) = stdout {
            pump(reader, slot.clone(), true);
        }
        if let Some(reader) = stderr {
            pump(reader, slot.clone(), false);
        }
        monitor(slot);
        Ok(())
    }

    fn send(&self, module_id: &str, command: &str) -> Result<(), ApiError> {
        let Some(slot) = self.slot(module_id) else {
            return Err(ApiError::new("shell.module_not_running"));
        };
        let mut guard = slot
            .lock()
            .map_err(|_| ApiError::new("shell.module_launch_failed").caused_by("run registry lock poisoned"))?;
        let Some(stdin) = guard.stdin.as_mut() else {
            return Err(ApiError::new("shell.module_not_running"));
        };
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|error| ApiError::new("shell.module_launch_failed").caused_by(error))?;
        guard.last_activity = std::time::Instant::now();
        Ok(())
    }

    fn status(&self, module_id: &str) -> ModuleStatus {
        let Some(slot) = self.slot(module_id) else {
            return ModuleStatus::unknown();
        };
        let Ok(guard) = slot.lock() else {
            return ModuleStatus::unknown();
        };
        let mut status = ModuleStatus {
            known: true,
            running: guard.running,
            stopped: guard.stopped,
            timed_out: guard.timed_out,
            idle_seconds: if guard.running { guard.last_activity.elapsed().as_secs() } else { 0 },
            exit_code: guard.exit_code,
            emitted: Vec::new(),
            stderr: guard.error.clone(),
            error: None,
        };
        let report = guard
            .output
            .lines()
            .rev()
            .find(|line| line.trim_start().starts_with('{'))
            .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok());
        if let Some(value) = report {
            status.exit_code = value
                .get("exit_code")
                .and_then(|value| value.as_i64())
                .map(|code| code as i32)
                .or(status.exit_code);
            status.timed_out = value
                .get("timed_out")
                .and_then(|value| value.as_bool())
                .unwrap_or(status.timed_out);
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
        } else if !guard.running && !guard.output.is_empty() {
            status.error = Some("launcher report unreadable".to_string());
        }
        status
    }

    fn stop(&self, module_id: &str) -> Result<(), ApiError> {
        let Some(slot) = self.slot(module_id) else {
            return Ok(());
        };
        let mut guard = slot
            .lock()
            .map_err(|_| ApiError::new("shell.module_launch_failed").caused_by("run registry lock poisoned"))?;
        if !guard.running {
            return Ok(());
        }
        let written = guard
            .stdin
            .as_mut()
            .map(|stdin| stdin.write_all(b"stop\n").and_then(|_| stdin.flush()));
        if !matches!(written, Some(Ok(()))) {
            if let Some(child) = guard.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            guard.child = None;
            guard.stdin = None;
            guard.running = false;
            guard.stopped = true;
            return Ok(());
        }
        guard.stopped = true;
        drop(guard);
        let watchdog = slot.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(5));
            if let Ok(mut guard) = watchdog.lock() {
                if guard.running {
                    if let Some(child) = guard.child.as_mut() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    guard.child = None;
                    guard.stdin = None;
                    guard.running = false;
                            guard.stopped = true;
                }
            }
        });
        Ok(())
    }

    fn kill_all(&self) {
        let slots: Vec<_> = self
            .runs
            .lock()
            .map(|runs| runs.values().cloned().collect())
            .unwrap_or_default();
        for slot in slots {
            if let Ok(mut guard) = slot.lock() {
                if let Some(child) = guard.child.as_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                guard.child = None;
                guard.stdin = None;
                guard.running = false;
                guard.stopped = true;
            }
        }
    }
}

impl Drop for ModuleRuns {
    fn drop(&mut self) {
        // 壳退出不走优雅控制：直接把子进程收掉，别留孤儿。
        self.kill_all();
    }
}

fn pump(mut reader: impl Read + Send + 'static, slot: Arc<Mutex<RunSlot>>, stdout: bool) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            let text = String::from_utf8_lossy(&buffer[..read]);
            if let Ok(mut guard) = slot.lock() {
                if stdout {
                    guard.output.push_str(&text);
                } else {
                    guard.error.push_str(&text);
                }
            }
        }
    });
}

fn monitor(slot: Arc<Mutex<RunSlot>>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(200));
        let Ok(mut guard) = slot.lock() else {
            return;
        };
        let Some(child) = guard.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                guard.exit_code = status.code();
                guard.running = false;
                guard.child = None;
                return;
            }
            Ok(None) => {}
            Err(error) => {
                guard.error.push_str(&format!("wait: {error}"));
                guard.running = false;
                guard.child = None;
                return;
            }
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
    if runs.is_running(&module_id) {
        runs.send(&module_id, "run")?;
        return Ok(ModuleRunStarted { module_id });
    }

    let host = resolve_bin("yanmo-host", "YANMO_HOST_BIN")?;
    let launcher = resolve_bin("yanmo-launch", "YANMO_LAUNCH_BIN")?;
    let mut command = Command::new(&launcher);
    command
        .arg("--db")
        .arg(data.db_path())
        .arg("--module")
        .arg(&managed)
        .arg("--host")
        .arg(&host)
        .arg("--timeout-secs")
        .arg("3600")
        .arg("--idle-secs")
        .arg("300")
        .arg("--resident")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = command.spawn().map_err(|error| {
        ApiError::new("shell.module_launch_failed").caused_by(error)
    })?;
    runs.start(&module_id, child)?;
    runs.send(&module_id, "run")?;
    Ok(ModuleRunStarted { module_id })
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_status(
    runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<ModuleStatus, ApiError> {
    Ok(runs.status(&module_id))
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_stop(runs: State<'_, ModuleRuns>, module_id: String) -> Result<(), ApiError> {
    runs.stop(&module_id)
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
