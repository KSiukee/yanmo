//! 网络/凭据使用审计：**只读宿主写的 JSONL**（域名、字节、凭据名、结果），不读正文/URL。

use std::fs;
use std::io::{Read, Seek, SeekFrom};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAuditRow {
    pub ts: u64,
    pub module: String,
    pub host: String,
    pub bytes: usize,
    pub credential: Option<String>,
    pub result: String,
}

#[tauri::command(rename_all = "snake_case")]
pub fn network_audit_recent(
    data: State<'_, AppData>,
    limit: usize,
    module: Option<String>,
    result: Option<String>,
) -> Result<Vec<NetworkAuditRow>, ApiError> {
    let limit = limit.clamp(1, 100);
    Ok(read_rows(&data, limit, module.as_deref(), result.as_deref()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn network_audit_export(
    data: State<'_, AppData>,
    module: Option<String>,
    result: Option<String>,
) -> Result<String, ApiError> {
    let rows = read_rows(&data, 1000, module.as_deref(), result.as_deref());
    Ok(serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".to_string()))
}

/// 一个模块的出网小结（**只数日志里有的记录**：日志轮转后旧的那份不在窗口里）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NetworkSummary {
    /// 这次扫过多少条日志（窗口大小，界面用来如实说「最近记录里」）。
    pub scanned: u64,
    /// 这个模块的出网次数。
    pub requests: u64,
    /// 成功（`ok`）的次数。
    pub ok: u64,
    /// 没成的次数（被拦 / 配额 / 凭据不匹配……结果里带的是错误码）。
    pub failed: u64,
    /// 成功请求的字节合计。
    pub bytes: u64,
    /// 最近一条的时刻（毫秒；0 = 没有记录）。
    pub last_at: u64,
    pub last_host: String,
    pub last_result: String,
}

/// 读日志尾部、按模块**精确**汇总（`read_rows` 的模块过滤是"包含"，这里不将就它）。
pub(crate) fn summary_for(data: &AppData, module: &str) -> NetworkSummary {
    let rows = read_rows(data, 1000, Some(module), None);
    summarize(&rows, module)
}

fn summarize(rows: &[NetworkAuditRow], module: &str) -> NetworkSummary {
    let mut summary = NetworkSummary { scanned: rows.len() as u64, ..NetworkSummary::default() };
    for row in rows.iter().filter(|row| row.module == module) {
        summary.requests += 1;
        if row.result == "ok" {
            summary.ok += 1;
            summary.bytes = summary.bytes.saturating_add(row.bytes as u64);
        } else {
            summary.failed += 1;
        }
        if row.ts >= summary.last_at {
            summary.last_at = row.ts;
            summary.last_host = row.host.clone();
            summary.last_result = row.result.clone();
        }
    }
    summary
}

fn read_rows(
    data: &AppData,
    limit: usize,
    module: Option<&str>,
    result: Option<&str>,
) -> Vec<NetworkAuditRow> {
    let path = data.data_dir().join("network-audit.jsonl");
    let Ok(metadata) = fs::metadata(&path) else {
        return Vec::new();
    };
    let mut text = String::new();
    if metadata.len() <= 512 * 1024 {
        if let Ok(raw) = fs::read_to_string(&path) {
            text = raw;
        }
    } else {
        // 只读尾部，避免日志越长越慢；丢掉第一行（可能被截断）。
        if let Ok(mut file) = fs::File::open(&path) {
            let start = metadata.len().saturating_sub(256 * 1024);
            if file.seek(SeekFrom::Start(start)).is_ok() {
                let mut raw = Vec::new();
                if file.read_to_end(&mut raw).is_ok() {
                    text = String::from_utf8_lossy(&raw).into_owned();
                    if let Some(index) = text.find('\n') {
                        text = text[index + 1..].to_string();
                    }
                }
            }
        }
    }
    let module = module.map(str::to_ascii_lowercase);
    let result = result.map(str::to_ascii_lowercase);
    let mut rows: Vec<NetworkAuditRow> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<NetworkAuditRow>(line).ok())
        .filter(|row| {
            module
                .as_deref()
                .is_none_or(|needle| row.module.to_ascii_lowercase().contains(needle))
                && result
                    .as_deref()
                    .is_none_or(|needle| row.result.to_ascii_lowercase().contains(needle))
        })
        .collect();
    if rows.len() > limit {
        rows = rows.split_off(rows.len() - limit);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(module: &str, host: &str, bytes: usize, result: &str, ts: u64) -> NetworkAuditRow {
        NetworkAuditRow {
            ts,
            module: module.to_string(),
            host: host.to_string(),
            bytes,
            credential: None,
            result: result.to_string(),
        }
    }

    #[test]
    fn summary_counts_only_the_module_it_was_asked_about() {
        let rows = vec![
            row("app.a", "api.a.com", 100, "ok", 10),
            row("app.a", "api.a.com", 0, "host.network_denied", 20),
            row("app.b", "api.b.com", 999, "ok", 30),
            row("app.a", "cdn.a.com", 5, "ok", 40),
        ];
        let summary = summarize(&rows, "app.a");
        assert_eq!(summary.scanned, 4);
        assert_eq!(summary.requests, 3);
        assert_eq!(summary.ok, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.bytes, 105);
        assert_eq!(summary.last_at, 40);
        assert_eq!(summary.last_host, "cdn.a.com");
    }

    #[test]
    fn a_module_without_records_says_zero() {
        let summary = summarize(&[], "app.a");
        assert_eq!(summary, NetworkSummary::default());
    }
}
