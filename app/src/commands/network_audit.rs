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
