//! 平台模式的**结果攒账**：把宿主每一行报告变成"每个模块的终局"。
//!
//! 宿主（及其每个模块的子进程）把报告打在 stdout / stderr 上；平台模式要
//! **边转发边攒**——壳看到的流不变，启动器的报告里多出"哪个模块成了、哪个没成、
//! 为什么"。失败要能指名道姓：只知道"坏了一个"、不知道坏的是谁，等于没报。

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use yanmo_core::store::ModuleData;

use crate::ModuleRun;

pub(crate) type Outcomes = Arc<Mutex<HashMap<String, ModuleRun>>>;

/// 接住宿主的 stdout / stderr：逐行转发给壳，同时攒终局。
/// 唤起留痕：每个模块**第一次出现结果**时记一行（谁唤起、多久、结果）。
///
/// 记在启动器这边，是因为它是唯一同时看得见"壳发了 run"与"模块出了结果"的进程。
pub(crate) struct RunRecorder {
    pub(crate) starts: Arc<Mutex<HashMap<String, (i64, Instant, String)>>>,
    pub(crate) data: Option<Arc<Mutex<ModuleData>>>,
}

impl RunRecorder {
    pub(crate) fn note(&self, module_id: &str, outcomes: &Outcomes) {
        let started = self
            .starts
            .lock()
            .ok()
            .and_then(|map| map.get(module_id).cloned());
        let Some((started_at, instant, trigger)) = started else { return };
        let (outcome, code) = {
            let Ok(map) = outcomes.lock() else { return };
            match map.get(module_id) {
                Some(run) if run.ok => ("ok".to_string(), String::new()),
                Some(run) => ("error".to_string(), run.code.clone().unwrap_or_default()),
                None => return,
            }
        };
        let duration = instant.elapsed().as_millis() as i64;
        self.write(module_id, started_at, duration, &outcome, &code, &trigger);
        if let Ok(mut map) = self.starts.lock() {
            map.remove(module_id);
        }
    }

    /// 收工时给"一行报告都没等到"的模块补一条：**沉默也是事实**（没跑完 / 超时）。
    pub(crate) fn note_unfinished(&self, timed_out: bool) {
        let leftover: Vec<(String, i64, i64)> = {
            let Ok(mut map) = self.starts.lock() else { return };
            map.drain()
                .map(|(id, (started_at, instant, _trigger))| {
                    (id, started_at, instant.elapsed().as_millis() as i64)
                })
                .collect()
        };
        let outcome = if timed_out { "timeout" } else { "unfinished" };
        for (module_id, started_at, duration) in leftover {
            self.write(&module_id, started_at, duration, outcome, "", "user");
        }
    }

    /// 被断链拒掉的唤起：留一行 `cut`，来源记得是**谁想唤起它**。
    pub(crate) fn note_cut(&self, module_id: &str, from: &str) {
        let started_at = self
            .starts
            .lock()
            .ok()
            .and_then(|map| map.get(module_id).map(|(at, _, _)| *at))
            .unwrap_or_else(yanmo_core::time::now_millis);
        self.write(module_id, started_at, 0, "cut", "module.chain_cut", &format!("chain:{from}"));
        if let Ok(mut map) = self.starts.lock() {
            map.remove(module_id);
        }
    }

    fn write(
        &self,
        module_id: &str,
        started_at: i64,
        duration: i64,
        outcome: &str,
        code: &str,
        trigger: &str,
    ) {
        if let Some(data) = &self.data {
            if let Ok(mut data) = data.lock() {
                let _ = data.note_run_result(module_id, trigger, started_at, duration, outcome, code);
            }
        }
    }
}

pub(crate) fn spawn_readers(
    child: &mut Child,
    recorder: Arc<RunRecorder>,
) -> Outcomes {
    let outcomes: Outcomes = Arc::new(Mutex::new(HashMap::new()));
    if let Some(stdout) = child.stdout.take() {
        let outcomes = outcomes.clone();
        let recorder = recorder.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(module_id) = collect_outcome(&outcomes, &line, false) {
                    recorder.note(&module_id, &outcomes);
                }
                let mut out = std::io::stdout();
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let outcomes = outcomes.clone();
        let recorder = recorder.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if let Some(module_id) = collect_outcome(&outcomes, &line, true) {
                    recorder.note(&module_id, &outcomes);
                }
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
fn collect_outcome(
    outcomes: &Mutex<HashMap<String, ModuleRun>>,
    line: &str,
    from_stderr: bool,
) -> Option<String> {
    let text = line.trim();
    if !text.starts_with('{') {
        return None;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return None;
    };
    let module_id = value.get("module_id").and_then(|id| id.as_str())?;
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
    let Ok(mut map) = outcomes.lock() else { return None };
    let fresh = !map.contains_key(module_id);
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
    fresh.then(|| module_id.to_string())
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

/// 收工前的最后一道账：**每个被请求过的模块都要有一个终局**。
///
/// 平台被强杀（壳关 stdin）或整体超时时，还没跑完的模块没有任何报告行；
/// 不补这一手，报告里就只剩"沉默"——比"失败"更糟，因为它不可归因。
pub(crate) fn finalize(
    mut modules: Vec<ModuleRun>,
    requested: &[String],
    timed_out: bool,
) -> Vec<ModuleRun> {
    let code = if timed_out { "launch.timed_out" } else { "launch.unfinished" };
    for module_id in requested {
        let id = module_id.trim();
        if id.is_empty() || modules.iter().any(|run| run.module_id == id) {
            continue;
        }
        modules.push(ModuleRun {
            module_id: id.to_string(),
            ok: false,
            code: Some(code.to_string()),
            memory_bytes: 0,
            fuel_used: 0,
        });
    }
    modules.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    modules.dedup_by(|left, right| left.module_id == right.module_id);
    modules
}

#[cfg(test)]
mod finalize_tests {
    use super::*;

    fn run(id: &str, ok: bool) -> ModuleRun {
        ModuleRun { module_id: id.to_string(), ok, code: None, memory_bytes: 0, fuel_used: 0 }
    }

    #[test]
    fn modules_that_never_reported_still_get_a_terminal_state() {
        let modules = finalize(vec![run("app.a", true)], &["app.a".into(), "app.b".into()], false);
        assert_eq!(modules.len(), 2);
        let b = modules.iter().find(|run| run.module_id == "app.b").unwrap();
        assert!(!b.ok);
        assert_eq!(b.code.as_deref(), Some("launch.unfinished"));
    }

    #[test]
    fn a_timeout_says_timed_out_not_unfinished() {
        let modules = finalize(Vec::new(), &["app.c".into()], true);
        assert_eq!(modules[0].code.as_deref(), Some("launch.timed_out"));
    }

    #[test]
    fn nothing_new_is_added_when_everything_reported() {
        let modules = finalize(vec![run("app.a", true)], &["app.a".into()], false);
        assert_eq!(modules.len(), 1);
        assert!(modules[0].ok, "已经成功的不许被改写");
    }
}
