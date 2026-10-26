//! 模块间唤起的**最小 ABI**：模块在报告里 `emit` 一条 `{"wake":"<模块 id>"}`，
//! 启动器替它把 `run <id> from:<自己>` 发给宿主——**过三道闸**：
//!
//! 1. **断链**：作者掐断的边一律不转发（`module_chain_cut` 表）；
//! 2. **深度**：链最长 3 层（用户点 → A → B → C 就到头，D 不许再叫）；
//! 3. **扇出**：一次平台会话最多 8 次唤起（防唤醒风暴）。
//!
//! 为什么闸放在启动器：它是唯一同时看得见"谁在跑"与"宿主 stdin"的地方，
//! 也是断链表所在的进程——**模块自己发的请求不构成信任**。

use std::collections::HashMap;
use std::io::Write;
use std::process::ChildStdin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use yanmo_core::store::ModuleData;

/// 链最多几层（含用户那次点击：点 → A → B → C）。
pub(crate) const MAX_DEPTH: u32 = 3;
/// 一次平台会话最多唤起几次（全场，不分模块）。
pub(crate) const MAX_WAKES: usize = 8;

/// 一次唤起的判定结果（稳定码进留痕与报告）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WakeVerdict {
    Sent,
    Cut,
    TooDeep,
    TooMany,
    BadId,
}

impl WakeVerdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WakeVerdict::Sent => "sent",
            WakeVerdict::Cut => "cut",
            WakeVerdict::TooDeep => "too_deep",
            WakeVerdict::TooMany => "too_many",
            WakeVerdict::BadId => "bad_id",
        }
    }
}

/// 唤起台：拿着宿主的 stdin、断链表与两张额度表。
pub(crate) struct WakeDesk {
    stdin: Arc<Mutex<ChildStdin>>,
    data: Option<Arc<Mutex<ModuleData>>>,
    depth: Arc<Mutex<HashMap<String, u32>>>,
    wakes: AtomicUsize,
    /// 与宿主侧共用的"起点表"：替它记下 B 从哪一刻开始跑（留痕要算时长）。
    starts: Arc<Mutex<HashMap<String, (i64, Instant, String)>>>,
}

impl WakeDesk {
    pub(crate) fn new(
        stdin: Arc<Mutex<ChildStdin>>,
        data: Option<Arc<Mutex<ModuleData>>>,
        starts: Arc<Mutex<HashMap<String, (i64, Instant, String)>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            stdin,
            data,
            depth: Arc::new(Mutex::new(HashMap::new())),
            wakes: AtomicUsize::new(0),
            starts,
        })
    }

    /// 记下"谁在第几层"（根 = 0）。
    pub(crate) fn note_depth(&self, module_id: &str, depth: u32) {
        if let Ok(mut map) = self.depth.lock() {
            map.entry(module_id.to_string()).or_insert(depth);
        }
    }

    fn depth_of(&self, module_id: &str) -> u32 {
        self.depth.lock().ok().and_then(|map| map.get(module_id).copied()).unwrap_or(0)
    }

    /// 一次唤起请求：能发就发，不能发给出**具体原因**（不许静默）。
    pub(crate) fn consider(&self, from: &str, to: &str) -> WakeVerdict {
        let to = to.trim();
        if to.is_empty() || to == from || to.len() > 120 {
            return WakeVerdict::BadId;
        }
        if let Some(data) = &self.data {
            if let Ok(data) = data.lock() {
                if data.is_cut(from, to).unwrap_or(false) {
                    return WakeVerdict::Cut;
                }
            }
        }
        let next_depth = self.depth_of(from) + 1;
        if next_depth > MAX_DEPTH {
            return WakeVerdict::TooDeep;
        }
        if self.wakes.load(Ordering::Relaxed) >= MAX_WAKES {
            return WakeVerdict::TooMany;
        }
        let Ok(mut stdin) = self.stdin.lock() else { return WakeVerdict::BadId };
        let line = format!("run {to} from:{from}\n");
        if stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).is_err() {
            return WakeVerdict::BadId;
        }
        self.wakes.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut map) = self.starts.lock() {
            map.entry(to.to_string()).or_insert_with(|| {
                (
                    yanmo_core::time::now_millis(),
                    Instant::now(),
                    format!("chain:{from}"),
                )
            });
        }
        if let Ok(mut map) = self.depth.lock() {
            map.insert(to.to_string(), next_depth);
        }
        WakeVerdict::Sent
    }
}

/// 从模块 `emit` 出来的字符串里挑出唤起请求。
pub(crate) fn wake_targets(emitted: &[String]) -> Vec<String> {
    emitted
        .iter()
        .filter_map(|item| {
            let value: serde_json::Value = serde_json::from_str(item.trim()).ok()?;
            value.get("wake")?.as_str().map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_json_with_a_wake_key_counts() {
        let emitted = vec![
            "{\"wake\":\"app.b\"}".to_string(),
            "{\"bytes\":856}".to_string(),
            "not json".to_string(),
        ];
        assert_eq!(wake_targets(&emitted), vec!["app.b".to_string()]);
    }
}
