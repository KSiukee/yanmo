//! 模块运行对界面暴露的两份快照：启动回执与状态。

use serde::Serialize;

/// 启动回执（真正结果用 `module_status` 取）。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRunStarted {
    pub module_id: String,
    /// 启动前那份镜像：`fresh`（本来就一致，没重写）/ `written`（这次写了）/ `failed`。
    pub mirror_state: String,
    /// `failed` 时的原因（其它时候是空）。
    pub mirror_error: Option<String>,
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

    /// 把宿主交回的一行报告**贴到这份快照上**（缺的字段按 false / 空算）。
    ///
    /// 报告的形状由宿主定（见 `yanmo-host` 的运行报告）；这里只做字段搬运，
    /// 好让「报告长什么样」与「平台怎么拉起来」留在两个文件里。
    pub(super) fn apply_report(&mut self, value: &serde_json::Value) {
        self.known = true;
        self.running = false;
        self.timed_out = value
            .get("timed_out")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.emitted = value
            .get("emitted")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        self.stderr = value
            .get("stderr")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        self.data_degraded = value
            .get("data_degraded")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.data_rejected = value
            .get("data_rejected")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        if value.get("ok").and_then(|value| value.as_bool()) == Some(false) {
            let code = value
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("module.run_failed");
            let detail = value
                .get("detail")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            self.error = Some(format!("{code}: {detail}"));
        }
    }
}
