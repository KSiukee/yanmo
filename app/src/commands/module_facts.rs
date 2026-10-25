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
use yanmo_core::store::{ModuleCard, ModuleData, ModuleFacts};

use super::module_run::{status_module, ModuleRuns};
use super::network_audit::{summary_for, NetworkSummary};
use crate::error::ApiError;
use crate::storage::AppData;

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
    let status = status_module(runs.inner(), &module_id)?;
    Ok(ModuleFactsView {
        card,
        facts,
        program_bytes,
        network,
        running: status.running,
        idle_seconds: status.idle_seconds,
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
