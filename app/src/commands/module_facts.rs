//! 模块事实页：把「空间 / 行为 / 越界 / 身份」拼成一份**只读**视图。
//!
//! 事实的来源分三处，界面必须能看出「谁说的」：
//! - **核心观测**：数据字节、表/行、跑过几次、被拒次数、隔离与降级（`ModuleData::facts`）；
//! - **壳观测**：程序文件占多大（壳才碰文件系统）、当前跑没在跑、出网日志汇总；
//! - **模块自报**：清单里声明的能力与说明（卡片里那几项），认证是核心验签后的结论。
//!
//! 这里只拼装，不做任何判断——「安不安全」是作者看完事实之后的判断。

use std::path::Path;

use serde::Serialize;
use tauri::State;
use yanmo_core::store::{
    recent_entries, summarize, ModuleCard, ModuleData, ModuleFacts, Store, READ_LOG_FILE,
};

use super::module_run::{status_module, ModuleRuns};
use super::network_audit::{summary_for, NetworkSummary};
use crate::error::ApiError;
use crate::storage::AppData;

/// 读账的窗口：**最近 24 小时**（不叫"今天"——本机时区的零点在壳里没有权威口径，
/// 与其猜，不如把窗口说死）。
const READ_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// 读过的书（面板上「《长夜》12 次」那一项）。
#[derive(Debug, Clone, Serialize)]
pub struct ReadBookView {
    pub work_id: i64,
    /// 书还在架上就带上名字；不在（删了）时是 `None`，界面按 id 说。
    pub title: Option<String>,
    pub reads: u64,
    pub chars: u64,
}

/// 留档：回滚的目标版本。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveView {
    pub version: String,
    pub archived_at: i64,
}

/// 一个模块最近 24 小时的读账小结（核心观测）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReadSummaryView {
    pub reads: u64,
    pub denied: u64,
    pub chars: u64,
    pub books: Vec<ReadBookView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleFactsView {
    /// 注册表里那一行（身份与声明）。
    pub card: ModuleCard,
    /// 核心观测到的数据事实。
    pub facts: ModuleFacts,
    /// 受管程序目录的字节数（壳观测；读不到按 0 算）。
    pub program_bytes: u64,
    /// 出网日志汇总（壳观测；只覆盖日志窗口里有的记录）。
    pub network: NetworkSummary,
    /// 读账汇总（核心观测；最近 24 小时）。
    pub reads: ReadSummaryView,
    /// 上一版的留档（有才能回滚；没有就是 `None`）。
    pub archive: Option<ArchiveView>,
    /// 现在跑没在跑 + 空闲多少秒（壳观测）。
    pub running: bool,
    pub idle_seconds: u64,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_facts(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
) -> Result<ModuleFactsView, ApiError> {
    let card = data
        .with_store(|store| store.module_cards())?
        .into_iter()
        .find(|card| card.id == module_id)
        .ok_or_else(|| {
            ApiError::from(yanmo_core::Error::invalid_with(
                yanmo_core::error_codes::codes::MODULE_NOT_FOUND,
                [("module_id", module_id.clone())],
            ))
        })?;
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    let facts = module_data.facts(&module_id)?;
    let program_bytes = dir_bytes(&data.data_dir().join("modules").join(&module_id));
    let network = summary_for(&data, &module_id);
    let reads = read_summary(&data, &module_id)?;
    let archive = yanmo_core::store::read_archive(
        &data.data_dir().join(yanmo_core::store::ARCHIVE_DIR),
        &module_id,
    )
    .map(|record| ArchiveView { version: record.version, archived_at: record.archived_at });
    let status = status_module(runs.inner(), &module_id)?;
    Ok(ModuleFactsView {
        card,
        facts,
        program_bytes,
        network,
        reads,
        archive,
        running: status.running,
        idle_seconds: status.idle_seconds,
    })
}

/// 最近 24 小时的读账（顺带把 work id 换成书名——书还在架上才换得到）。
fn read_summary(data: &AppData, module_id: &str) -> Result<ReadSummaryView, ApiError> {
    let entries = recent_entries(&data.data_dir().join(READ_LOG_FILE), 5000);
    let since = yanmo_core::time::now_millis() - READ_WINDOW_MS;
    let summary = summarize(&entries, module_id, since);
    let titles: std::collections::HashMap<i64, String> = data
        .with_store(|store: &mut Store| store.shelf())?
        .into_iter()
        .map(|entry| (entry.work.id, entry.work.title))
        .collect();
    Ok(ReadSummaryView {
        reads: summary.reads,
        denied: summary.denied,
        chars: summary.chars,
        books: summary
            .works
            .into_iter()
            .map(|work| ReadBookView {
                work_id: work.work_id,
                title: titles.get(&work.work_id).cloned(),
                reads: work.reads,
                chars: work.chars,
            })
            .collect(),
    })
}

/// 目录里所有普通文件的字节合计（读不到的项算 0：这一项是读数，不该拦住整页）。
fn dir_bytes(root: &Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                total = total.saturating_add(entry.metadata().map(|meta| meta.len()).unwrap_or(0));
            }
        }
    }
    total
}
