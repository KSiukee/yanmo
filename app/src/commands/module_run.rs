//! 模块运行：**后台拉起独立启动器**，管理页能看状态、能停。
//!
//! 壳不链接沙箱/IPC；这里只做三件事：核对受管包指纹、拉起 `yanmo-launch`、记录子进程。
//! 停止 = 杀掉启动器进程；启动器的作业对象是 kill-on-close，宿主会跟着退。

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::Store;

/// 一次运行在壳里的登记（子进程 + 已收到的输出）。
struct RunSlot {
    child: Option<Child>,
    output: String,
    error: String,
    running: bool,
    finished: bool,
    stopped: bool,
    exit_code: Option<i32>,
    timed_out: bool,
}

impl Default for RunSlot {
    fn default() -> Self {
        Self {
            child: None,
            output: String::new(),
            error: String::new(),
            running: false,
            finished: false,
            stopped: false,
            exit_code: None,
            timed_out: false,
        }
    }
}

/// 运行登记表：按模块 id 保存**当前这一次**运行；同一模块同时只跑一个。
#[derive(Default)]
pub struct ModuleRuns {
    runs: Arc<Mutex<HashMap<String, Arc<Mutex<RunSlot>>>>>,
}

/// 启动回执（真正结果用 `module_status` 取）。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRunStarted {
    pub module_id: String,
}

/// 一次运行的状态快照。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleStatus {
    pub known: bool,
    pub running: bool,
    pub stopped: bool,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub emitted: Vec<String>,
    pub stderr: String,
    pub error: Option<String>,
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
        let slot = Arc::new(Mutex::new(RunSlot {
            child: Some(child),
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

    fn status(&self, module_id: &str) -> ModuleStatus {
        let Some(slot) = self.slot(module_id) else {
            return ModuleStatus::unknown();
        };
        let Ok(guard) = slot.lock() else {
            return ModuleStatus::unknown();
        };
        if guard.running {
            return ModuleStatus {
                known: true,
                running: true,
                stopped: guard.stopped,
                timed_out: false,
                exit_code: None,
                emitted: Vec::new(),
                stderr: String::new(),
                error: None,
            };
        }
        let mut status = ModuleStatus {
            known: true,
            running: false,
            stopped: guard.stopped,
            timed_out: guard.timed_out,
            exit_code: guard.exit_code,
            emitted: Vec::new(),
            stderr: guard.error.clone(),
            error: None,
        };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&guard.output) {
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
        } else if !guard.output.is_empty() {
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
        if let Some(child) = guard.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        guard.child = None;
        guard.running = false;
        guard.finished = true;
        guard.stopped = true;
        Ok(())
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self
            .runs
            .lock()
            .map(|runs| runs.keys().cloned().collect())
            .unwrap_or_default();
        for id in ids {
            let _ = self.stop(&id);
        }
    }
}

impl Drop for ModuleRuns {
    fn drop(&mut self) {
        self.stop_all();
    }
}

impl ModuleStatus {
    fn unknown() -> Self {
        Self {
            known: false,
            running: false,
            stopped: false,
            timed_out: false,
            exit_code: None,
            emitted: Vec::new(),
            stderr: String::new(),
            error: None,
        }
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
                guard.finished = true;
                guard.child = None;
                return;
            }
            Ok(None) => {}
            Err(error) => {
                guard.error.push_str(&format!("wait: {error}"));
                guard.running = false;
                guard.finished = true;
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
    if runs.is_running(&module_id) {
        return Err(ApiError::new("shell.module_already_running"));
    }
    let record = data.with_store(|store: &mut Store| store.module_record(&module_id))?;
    if !record.is_active() {
        return Err(ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::MODULE_DISABLED,
            [("module_id", module_id.clone())],
        )));
    }
    let managed = data.data_dir().join("modules").join(&module_id);
    data.with_store(|store: &mut Store| store.verify_module_package(&module_id, &managed))?;

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
