//! 模块运行对界面暴露的两份快照：启动回执与状态。

use serde::Serialize;

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
    pub idle_seconds: u64,
    pub exit_code: Option<i32>,
    pub emitted: Vec<String>,
    pub stderr: String,
    pub error: Option<String>,
    pub data_degraded: bool,
    pub data_rejected: usize,
}

impl ModuleStatus {
    pub(super) fn unknown() -> Self {
        Self {
            known: false,
            running: false,
            stopped: false,
            timed_out: false,
            idle_seconds: 0,
            exit_code: None,
            emitted: Vec::new(),
            stderr: String::new(),
            error: None,
            data_degraded: false,
            data_rejected: 0,
        }
    }
}
