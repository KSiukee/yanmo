//! 链路面板：**走过的边 + 断没断**，以及断/接两个动作。
//!
//! 边只从唤起留痕汇总（没走过就没有线）；断链写 `module_chain_cut`，
//! **重启还在**——这与断链的语义一致：防骚扰要彻底。

use serde::Serialize;
use tauri::State;
use yanmo_core::store::{ModuleData, Store};

use crate::error::ApiError;
use crate::storage::AppData;

#[derive(Debug, Clone, Serialize)]
pub struct ChainEdgeView {
    pub from: String,
    pub to: String,
    /// 两端在注册表里的名字（查不到就回 id，界面不空着）
    pub from_name: String,
    pub to_name: String,
    pub runs: i64,
    pub last_at: i64,
    pub failures: i64,
    pub cut: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_edges(data: State<'_, AppData>) -> Result<Vec<ChainEdgeView>, ApiError> {
    let module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let edges = module_data.edges()?;
    let names = data.with_store(|store: &mut Store| {
        let mut map = std::collections::HashMap::new();
        for card in store.module_cards()? {
            map.insert(card.id.clone(), card.name.clone());
        }
        Ok(map)
    })?;
    Ok(edges
        .into_iter()
        .map(|edge| ChainEdgeView {
            from_name: names.get(&edge.from).cloned().unwrap_or_else(|| edge.from.clone()),
            to_name: names.get(&edge.to).cloned().unwrap_or_else(|| edge.to.clone()),
            from: edge.from,
            to: edge.to,
            runs: edge.runs,
            last_at: edge.last_at,
            failures: edge.failures,
            cut: edge.cut,
        })
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_edge_cut(
    data: State<'_, AppData>,
    from: String,
    to: String,
) -> Result<(), ApiError> {
    let mut module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    module_data.cut_edge(&from, &to)?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_edge_restore(
    data: State<'_, AppData>,
    from: String,
    to: String,
) -> Result<bool, ApiError> {
    let mut module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    Ok(module_data.restore_edge(&from, &to)?)
}
