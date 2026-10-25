//! 模块读取留痕：**谁、什么时候、读了哪本书、读了多少**。
//!
//! # 为什么是一行一条的 JSONL
//!
//! 跟网络审计同一个做法、同一个目录、同一种轮转：
//! - **不写进稿子库**——这不是稿子，是一份给人看的账；备份/搬库时不必带上它；
//! - 一行一次读（`ok`）或一次被拒（`denied` + 错误码）——**被拒也是事实**，
//!   面板要能说"它试过，被拦住了"，而不是"从没发生过"。
//!
//! # 它回答什么问题
//!
//! 「这个模块今天读了哪几本书、读了多少字」「有没有被拒过」。
//! 判据（跨书 + 短时高频）与渐进放行是后面的事——这一层只管**事实**。
//!
//! # 失败不许打扰读
//!
//! 写账失败（磁盘满 / 没权限）只丢这一行，**绝不挡住读**：记账是旁路，读是正事。
//! 这一条写在 [`crate::store::ModuleIpcHandler`] 的调用点上，这里只回报 `io::Result`。

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 日志文件名（在数据目录里，与 `network-audit.jsonl` 并列）。
pub const READ_LOG_FILE: &str = "read-audit.jsonl";

/// 超过这个大小就轮转（只留当前与上一份；与网络审计同一条口径）。
const MAX_BYTES: u64 = 1024 * 1024;
/// 读尾部时最多看这么多字节（日志再大也不会把整份读进内存）。
const TAIL_BYTES: u64 = 256 * 1024;

/// 一次读（或被拒）的记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLogEntry {
    /// 时刻（毫秒时间戳）。
    pub ts: i64,
    /// 谁读的（模块 id）。
    pub module: String,
    /// 读的哪一块：`works` / `tree` / `body` / `outline` / `fragments` / `entities` /
    /// `scenes` / `foreshadows`。
    pub area: String,
    /// 落在哪本书上（`works` 这种"列个清单"定位不到，就是 `None`）。
    pub work: Option<i64>,
    /// 有意义的字数：**正文＝正文字数**；其它区域记 0（"读了多少字"说的是稿子里的字，
    /// 不是 JSON 的长度——体积另有 `bytes`）。
    pub chars: u64,
    /// 返回的字节数（体积口径；被拒时是 0）。
    pub bytes: u64,
    /// `ok` / `denied`。
    pub outcome: String,
    /// 被拒时的错误码。
    pub code: Option<String>,
}

impl ReadLogEntry {
    /// 读成功了一条。
    pub fn ok(module: &str, area: &str, work: Option<i64>, chars: u64, bytes: u64) -> Self {
        Self {
            ts: crate::time::now_millis(),
            module: module.to_string(),
            area: area.to_string(),
            work,
            chars,
            bytes,
            outcome: "ok".to_string(),
            code: None,
        }
    }

    /// 被门卫拦下（冻结 / 没授予）——这也是要留痕的事实。
    pub fn denied(module: &str, area: &str, work: Option<i64>, code: &str) -> Self {
        Self {
            ts: crate::time::now_millis(),
            module: module.to_string(),
            area: area.to_string(),
            work,
            chars: 0,
            bytes: 0,
            outcome: "denied".to_string(),
            code: Some(code.to_string()),
        }
    }

    pub fn allowed(&self) -> bool {
        self.outcome == "ok"
    }
}

/// 追加写一份读账（多进程各持一个句柄也安全：每条都是一次 `O_APPEND` 写）。
#[derive(Debug, Clone)]
pub struct ReadLog {
    path: PathBuf,
}

impl ReadLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 数据目录里的默认落点。
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(READ_LOG_FILE))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 记一条。失败只回报给调用方决定（读路径上会忽略它）。
    pub fn record(&self, entry: &ReadLogEntry) -> std::io::Result<()> {
        if std::fs::metadata(&self.path).map(|meta| meta.len() >= MAX_BYTES).unwrap_or(false) {
            let _ = std::fs::rename(&self.path, self.path.with_extension("1.jsonl"));
        }
        let line = serde_json::to_string(entry)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(file, "{line}")
    }
}

/// 读日志尾部（最多 `limit` 条；大文件只读最后 256 KB）。
pub fn recent_entries(path: &Path, limit: usize) -> Vec<ReadLogEntry> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Vec::new();
    };
    let text = if metadata.len() <= TAIL_BYTES {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        use std::io::{Read, Seek, SeekFrom};
        let mut text = String::new();
        if let Ok(mut file) = std::fs::File::open(path) {
            let start = metadata.len().saturating_sub(TAIL_BYTES);
            if file.seek(SeekFrom::Start(start)).is_ok() {
                let mut raw = Vec::new();
                if file.read_to_end(&mut raw).is_ok() {
                    text = String::from_utf8_lossy(&raw).into_owned();
                    // 丢掉被截断的第一行
                    if let Some(index) = text.find('\n') {
                        text = text[index + 1..].to_string();
                    }
                }
            }
        }
        text
    };
    let mut entries: Vec<ReadLogEntry> =
        text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
    if entries.len() > limit {
        entries = entries.split_off(entries.len() - limit);
    }
    entries
}

/// 读过的某本书（面板上"今天读过《长夜》12 次"就是它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkReads {
    pub work_id: i64,
    pub reads: u64,
    pub chars: u64,
}

/// 一段时间里一个模块的读账小结（纯统计，不含判断）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReadSummary {
    pub reads: u64,
    pub denied: u64,
    pub chars: u64,
    pub bytes: u64,
    /// 读过的书，按次数从多到少（同次数按 id 升序）。
    pub works: Vec<WorkReads>,
}

/// 统计一个模块从 `since_ms` 起的读账。
pub fn summarize(entries: &[ReadLogEntry], module: &str, since_ms: i64) -> ReadSummary {
    let mut summary = ReadSummary::default();
    let mut works: BTreeMap<i64, WorkReads> = BTreeMap::new();
    for entry in entries.iter().filter(|entry| entry.module == module && entry.ts >= since_ms) {
        if entry.allowed() {
            summary.reads += 1;
            summary.chars = summary.chars.saturating_add(entry.chars);
            summary.bytes = summary.bytes.saturating_add(entry.bytes);
            if let Some(work_id) = entry.work {
                let row = works
                    .entry(work_id)
                    .or_insert(WorkReads { work_id, reads: 0, chars: 0 });
                row.reads += 1;
                row.chars = row.chars.saturating_add(entry.chars);
            }
        } else {
            summary.denied += 1;
        }
    }
    summary.works = works.into_values().collect();
    summary.works.sort_by(|left, right| {
        right.reads.cmp(&left.reads).then(left.work_id.cmp(&right.work_id))
    });
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(module: &str, work: Option<i64>, chars: u64, ts: i64) -> ReadLogEntry {
        ReadLogEntry {
            ts,
            module: module.to_string(),
            area: "body".to_string(),
            work,
            chars,
            bytes: chars * 3,
            outcome: "ok".to_string(),
            code: None,
        }
    }

    #[test]
    fn summary_counts_only_the_module_and_the_window() {
        let rows = vec![
            entry("app.a", Some(1), 100, 10),
            entry("app.a", Some(1), 50, 20),
            entry("app.a", Some(2), 10, 30),
            entry("app.b", Some(1), 999, 40),
            entry("app.a", Some(3), 7, 5), // 窗口之前
        ];
        let summary = summarize(&rows, "app.a", 10);
        assert_eq!(summary.reads, 3);
        assert_eq!(summary.chars, 160);
        assert_eq!(summary.works.len(), 2, "窗口外那本不算");
        assert_eq!(summary.works[0].work_id, 1);
        assert_eq!(summary.works[0].reads, 2);
        assert_eq!(summary.works[1].work_id, 2);
    }

    #[test]
    fn denials_are_counted_but_not_as_reads() {
        let mut denied = entry("app.a", Some(1), 0, 15);
        denied.outcome = "denied".to_string();
        denied.code = Some("module.capability_denied".to_string());
        let rows = vec![entry("app.a", Some(1), 10, 10), denied];
        let summary = summarize(&rows, "app.a", 0);
        assert_eq!(summary.reads, 1);
        assert_eq!(summary.denied, 1);
        assert_eq!(summary.chars, 10);
    }

    #[test]
    fn a_round_trip_through_the_file_keeps_every_field() {
        let dir = tempfile::tempdir().unwrap();
        let log = ReadLog::new(dir.path().join(READ_LOG_FILE));
        let mut denied = ReadLogEntry::denied("app.a", "body", Some(9), "module.disabled");
        denied.ts = 42;
        log.record(&entry("app.a", Some(1), 12, 7)).unwrap();
        log.record(&denied).unwrap();

        let rows = recent_entries(log.path(), 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].chars, 12);
        assert_eq!(rows[1].outcome, "denied");
        assert_eq!(rows[1].code.as_deref(), Some("module.disabled"));
        assert_eq!(rows[1].work, Some(9));
        assert_eq!(rows[1].ts, 42);
    }
}
