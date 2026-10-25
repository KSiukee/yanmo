//! 平台模式的**结果攒账**：把宿主每一行报告变成"每个模块的终局"。
//!
//! 宿主（及其每个模块的子进程）把报告打在 stdout / stderr 上；平台模式要
//! **边转发边攒**——壳看到的流不变，启动器的报告里多出"哪个模块成了、哪个没成、
//! 为什么"。失败要能指名道姓：只知道"坏了一个"、不知道坏的是谁，等于没报。

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::process::Child;
use std::sync::{Arc, Mutex};

use crate::ModuleRun;

pub(crate) type Outcomes = Arc<Mutex<HashMap<String, ModuleRun>>>;

/// 接住宿主的 stdout / stderr：逐行转发给壳，同时攒终局。
pub(crate) fn spawn_readers(child: &mut Child) -> Outcomes {
    let outcomes: Outcomes = Arc::new(Mutex::new(HashMap::new()));
    if let Some(stdout) = child.stdout.take() {
        let outcomes = outcomes.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                collect_outcome(&outcomes, &line, false);
                let mut out = std::io::stdout();
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let outcomes = outcomes.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                collect_outcome(&outcomes, &line, true);
                let mut err = std::io::stderr();
                let _ = writeln!(err, "{line}");
                let _ = err.flush();
            }
        });
    }
    outcomes
}

/// 收工时把账结出来：按 id 排序 + 峰值内存。
pub(crate) fn summarize(outcomes: &Outcomes) -> (Vec<ModuleRun>, u64) {
    let map = outcomes.lock().map(|map| map.clone()).unwrap_or_default();
    let mut modules: Vec<ModuleRun> = map.into_values().collect();
    modules.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    let peak = modules.iter().map(|run| run.memory_bytes).max().unwrap_or(0);
    (modules, peak)
}

/// 从宿主的一行输出里认出一份"模块终局"。
///
/// 成功：宿主子进程跑完打的 RunReport（`{"module_id":…, "status":{"kind":"ok"}, …}`）。
/// 失败：平台自己的错（`{"ok":false,"module_id":…,"code":"host.spawn_failed"}`）。
fn collect_outcome(outcomes: &Mutex<HashMap<String, ModuleRun>>, line: &str, from_stderr: bool) {
    let text = line.trim();
    if !text.starts_with('{') {
        return;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    let Some(module_id) = value.get("module_id").and_then(|id| id.as_str()) else {
        return;
    };
    let ok = value.get("ok").and_then(|ok| ok.as_bool()).unwrap_or(false);
    let kind = value
        .get("status")
        .and_then(|status| status.get("kind"))
        .and_then(|kind| kind.as_str())
        .unwrap_or("");
    let succeeded = ok && (kind.is_empty() || kind == "ok") && !from_stderr;
    let code = value.get("code").and_then(|code| code.as_str()).map(str::to_string);
    let memory_bytes = value.get("memory_bytes").and_then(|bytes| bytes.as_u64()).unwrap_or(0);
    let fuel_used = value.get("fuel_used").and_then(|fuel| fuel.as_u64()).unwrap_or(0);
    let Ok(mut map) = outcomes.lock() else { return };
    let entry = map.entry(module_id.to_string()).or_insert_with(|| ModuleRun {
        module_id: module_id.to_string(),
        ok: true,
        code: None,
        memory_bytes: 0,
        fuel_used: 0,
    });
    // 一次运行可能先来 stderr 的错、再来 stdout 的报告：**失败优先**，别被后到的冲掉。
    if succeeded && entry.code.is_none() {
        entry.ok = true;
    } else if !succeeded {
        entry.ok = false;
        if entry.code.is_none() {
            entry.code = code;
        }
    }
    entry.memory_bytes = entry.memory_bytes.max(memory_bytes);
    entry.fuel_used = entry.fuel_used.max(fuel_used);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_report_becomes_a_named_success() {
        let outcomes = Mutex::new(HashMap::new());
        collect_outcome(
            &outcomes,
            r#"{"module_id":"app.a","ok":true,"status":{"kind":"ok"},"memory_bytes":4096,"fuel_used":7}"#,
            false,
        );
        let map = outcomes.lock().unwrap();
        let run = map.get("app.a").unwrap();
        assert!(run.ok);
        assert_eq!(run.memory_bytes, 4096);
        assert_eq!(run.fuel_used, 7);
        assert!(run.code.is_none());
    }

    #[test]
    fn a_platform_error_becomes_a_named_failure() {
        let outcomes = Mutex::new(HashMap::new());
        collect_outcome(
            &outcomes,
            r#"{"ok":false,"module_id":"app.b","code":"host.spawn_failed"}"#,
            true,
        );
        let map = outcomes.lock().unwrap();
        let run = map.get("app.b").unwrap();
        assert!(!run.ok);
        assert_eq!(run.code.as_deref(), Some("host.spawn_failed"));
    }

    #[test]
    fn a_late_report_does_not_erase_an_earlier_failure() {
        let outcomes = Mutex::new(HashMap::new());
        collect_outcome(
            &outcomes,
            r#"{"ok":false,"module_id":"app.c","code":"host.trap"}"#,
            true,
        );
        collect_outcome(
            &outcomes,
            r#"{"module_id":"app.c","ok":true,"status":{"kind":"ok"}}"#,
            false,
        );
        let map = outcomes.lock().unwrap();
        let run = map.get("app.c").unwrap();
        assert!(!run.ok, "失败不能被后到的成功冲掉");
        assert_eq!(run.code.as_deref(), Some("host.trap"));
    }
}
