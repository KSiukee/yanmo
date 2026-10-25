//! 模块的**提案**（正文补丁）：模块只提建议，**作者点一下才落盘**。
//!
//! 三条纪律：
//! 1. 提案是模块写在**自己数据区**里的 `proposals.json`，核心不读它、也不执行它；
//! 2. 应用走核心的 [`Store::apply_text_proposal`]：**哈希对不上就拒**（基于旧版本的
//!    建议不许静默套用）；
//! 3. 应用与被拒都记一笔（`proposal_results.json`）——"哪个模块老提垃圾建议"要能查。

use serde::Serialize;
use tauri::State;
use yanmo_core::store::{ModuleData, ProposalOutcome, Store, TreeChange};

use crate::error::ApiError;
use crate::storage::AppData;

/// 一条正文提案（模块提的，作者审的）。
#[derive(Debug, Clone, Serialize)]
pub struct ProposalView {
    pub id: String,
    pub node_id: i64,
    /// 模块写的标题（"改错字"这种）
    pub title: String,
    /// 提案基于的那一版哈希
    pub base_hash: String,
    /// 稿子已经变过 = 这条建议失效（界面要标出来，不许静默套用）
    pub stale: bool,
    /// 建议的正文长度（字数）；树提案是标题字数
    pub chars: u64,
    /// `text`（正文补丁）/ `rename`（改名）/ `move`（移动）
    pub kind: String,
    /// 树提案的落点（移动）
    pub parent: Option<i64>,
    pub index: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalsView {
    pub items: Vec<ProposalView>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_proposals(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ProposalsView, ApiError> {
    let items = read_proposals(&data, &module_id)?;
    if items.is_empty() {
        return Ok(ProposalsView { items: Vec::new() });
    }
    let current: Vec<(i64, Option<String>)> = data.with_store(|store: &mut Store| {
        Ok(items
            .iter()
            .map(|item| {
                let hash = if item.kind == "text" {
                    store.body_hash(item.node_id).ok().flatten()
                } else {
                    store.tree_fingerprint(item.node_id).ok().flatten()
                };
                (item.node_id, hash)
            })
            .collect::<Vec<_>>())
    })?;
    let items = items
        .into_iter()
        .map(|mut item| {
            let now = current
                .iter()
                .find(|(node_id, _)| *node_id == item.node_id)
                .and_then(|(_, hash)| hash.clone());
            item.stale = match now {
                Some(hash) => hash != item.base_hash,
                None => true, // 段没了：这条也没法应用
            };
            item
        })
        .collect();
    Ok(ProposalsView { items })
}

/// 应用一条提案：核心验哈希 → 落盘 → 记一笔结果。
#[tauri::command(rename_all = "snake_case")]
pub fn module_proposal_apply(
    data: State<'_, AppData>,
    module_id: String,
    proposal_id: String,
) -> Result<ProposalOutcome, ApiError> {
    let Some(proposal) = read_proposals(&data, &module_id)?
        .into_iter()
        .find(|item| item.id == proposal_id)
    else {
        return Err(ApiError::new("shell.module_proposal_missing"));
    };
    let outcome = match proposal.kind.as_str() {
        "rename" | "move" => {
            let change = if proposal.kind == "rename" {
                TreeChange::Rename { title: proposal.title.clone() }
            } else {
                TreeChange::Move {
                    parent: proposal.parent,
                    index: proposal.index.max(0) as usize,
                }
            };
            data.with_store(|store: &mut Store| {
                store.apply_tree_proposal(proposal.node_id, &proposal.base_hash, &change)
            })?
        }
        _ => {
            let body = read_proposal_body(&data, &module_id, &proposal_id)?;
            data.with_store(|store: &mut Store| {
                store.apply_text_proposal(proposal.node_id, &proposal.base_hash, &body)
            })?
        }
    };
    record_result(&data, &module_id, &proposal_id, &outcome)?;
    Ok(outcome)
}

/// 作者不要这条建议（也要留痕：被拒几次是能查的）。
#[tauri::command(rename_all = "snake_case")]
pub fn module_proposal_dismiss(
    data: State<'_, AppData>,
    module_id: String,
    proposal_id: String,
) -> Result<(), ApiError> {
    record_result(&data, &module_id, &proposal_id, &ProposalOutcome::Missing)?;
    Ok(())
}

fn read_proposals(data: &AppData, module_id: &str) -> Result<Vec<ProposalView>, ApiError> {
    let module_data = ModuleData::open(
        ModuleData::path_beside(&data.data_dir().join("yanmo.db")),
    )?;
    let Some(bytes) = module_data.kv_get(module_id, "proposals.json")? else {
        return Ok(Vec::new());
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Ok(Vec::new());
    };
    let items = document
        .get("items")
        .and_then(|value| value.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(|value| value.as_str())?.to_string();
                    let node_id = item.get("node_id").and_then(|value| value.as_i64())?;
                    let body = item.get("body").and_then(|value| value.as_str()).unwrap_or("");
                    let kind = item
                        .get("kind")
                        .and_then(|value| value.as_str())
                        .unwrap_or("text")
                        .to_string();
                    let title = item
                        .get("title")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    let chars = if kind == "text" {
                        body.chars().count() as u64
                    } else {
                        title.chars().count() as u64
                    };
                    Some(ProposalView {
                        id,
                        node_id,
                        title: title.to_string(),
                        base_hash: item
                            .get("base_hash")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .to_string(),
                        stale: false,
                        chars,
                        kind,
                        parent: item.get("parent").and_then(|value| value.as_i64()),
                        index: item.get("index").and_then(|value| value.as_i64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(items)
}

fn read_proposal_body(
    data: &AppData,
    module_id: &str,
    proposal_id: &str,
) -> Result<String, ApiError> {
    let module_data = ModuleData::open(
        ModuleData::path_beside(&data.data_dir().join("yanmo.db")),
    )?;
    let Some(bytes) = module_data.kv_get(module_id, "proposals.json")? else {
        return Err(ApiError::new("shell.module_proposal_missing"));
    };
    let document: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| ApiError::new("shell.module_proposal_invalid").caused_by(error))?;
    let body = document
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|list| {
            list.iter().find(|item| {
                item.get("id").and_then(|value| value.as_str()) == Some(proposal_id)
            })
        })
        .and_then(|item| item.get("body"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| ApiError::new("shell.module_proposal_missing"))?;
    Ok(body.to_string())
}

fn record_result(
    data: &AppData,
    module_id: &str,
    proposal_id: &str,
    outcome: &ProposalOutcome,
) -> Result<(), ApiError> {
    let mut module_data = ModuleData::open(
        ModuleData::path_beside(&data.data_dir().join("yanmo.db")),
    )?;
    let mut results: Vec<serde_json::Value> = module_data
        .kv_get(module_id, "proposal_results.json")
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let status = match outcome {
        ProposalOutcome::Applied { .. } => "applied",
        ProposalOutcome::Stale { .. } => "stale",
        ProposalOutcome::Missing => "dismissed",
    };
    results.push(serde_json::json!({
        "proposal_id": proposal_id,
        "status": status,
        "at": yanmo_core::time::now_millis(),
    }));
    // 只留最近 200 条：这是账，不是档案。
    if results.len() > 200 {
        let drop = results.len() - 200;
        results.drain(0..drop);
    }
    let text = serde_json::to_vec(&serde_json::Value::Array(results))
        .map_err(|error| ApiError::new("shell.module_proposal_invalid").caused_by(error))?;
    module_data.kv_put(module_id, "proposal_results.json", &text)?;
    Ok(())
}
