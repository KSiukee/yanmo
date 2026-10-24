//! 核心侧的模块 IPC **只读能力表**：把 `ModuleReadApi` 接成 `ReadHandler`。
//!
//! 传输（回环 TCP / token / 帧）在独立的 `yanmo-ipc` crate 里——**核心与协议层不含
//! `std::net`**，所以"出货零出网"的边界不被模块平台破掉。启动器（壳/守护进程）负责
//! `bind_loopback`、开一个独立 `Store` 连接、把 [`ModuleIpcHandler`] 交给
//! `yanmo_ipc::serve_one`；宿主断开就结束。
//!
//! 每个请求按 [`CoreDataArea`] 走 [`super::ModuleReadApi`] 的门卫（冻结 / 授予 / 只读），
//! 返回 **JSON 字符串**，不返回任何写句柄或 SQL 入口。

use serde_json::{json, Value};
use yanmo_proto::capability::CoreDataArea;
use yanmo_proto::ipc::{IpcError, ReadHandler};

use super::{ModuleOutlineRow, NodeSummary, ShelfEntry, Store};
use crate::error::{codes, Error, Result};

/// 把一个真实 `Store` 的只读视图接在 IPC 上。
pub struct ModuleIpcHandler {
    store: Store,
    module_id: String,
}

impl ModuleIpcHandler {
    pub fn new(store: Store, module_id: impl Into<String>) -> Self {
        Self { store, module_id: module_id.into() }
    }
}

impl ReadHandler for ModuleIpcHandler {
    fn read(&mut self, area: CoreDataArea, arg: Option<&str>) -> std::result::Result<String, IpcError> {
        let reader = self.store.module_reader(&self.module_id).map_err(to_ipc)?;
        match area {
            CoreDataArea::Works => {
                let entries = reader.works().map_err(to_ipc)?;
                to_json(entries.iter().map(work_json).collect::<Vec<_>>())
            }
            CoreDataArea::Tree => {
                let work_id = id_arg(arg)?;
                let nodes = reader.tree(work_id).map_err(to_ipc)?;
                to_json(nodes.iter().map(node_json).collect::<Vec<_>>())
            }
            CoreDataArea::Body => {
                let node_id = id_arg(arg)?;
                let body = reader.body(node_id).map_err(to_ipc)?;
                to_json(body)
            }
            CoreDataArea::Outline => {
                let work_id = id_arg(arg)?;
                let rows = reader.outline(work_id).map_err(to_ipc)?;
                to_json(rows.iter().map(outline_json).collect::<Vec<_>>())
            }
            CoreDataArea::Fragments => {
                let work_id = id_arg(arg)?;
                let fragments = reader.fragments(work_id).map_err(to_ipc)?;
                to_json(fragments)
            }
            CoreDataArea::Entities => {
                let work_id = id_arg(arg)?;
                let cards = reader.entities(work_id).map_err(to_ipc)?;
                to_json(cards)
            }
            CoreDataArea::Scenes => {
                let work_id = id_arg(arg)?;
                let scenes = reader.scenes(work_id).map_err(to_ipc)?;
                to_json(
                    scenes
                        .iter()
                        .map(|(node_id, title, fields)| {
                            json!({ "node_id": node_id, "title": title, "fields": fields })
                        })
                        .collect::<Vec<Value>>(),
                )
            }
            CoreDataArea::Foreshadows => {
                let work_id = id_arg(arg)?;
                let rows = reader.foreshadows(work_id).map_err(to_ipc)?;
                to_json(rows)
            }
        }
    }
}

/// 一次性 token：32 字节系统随机数 → 64 位十六进制。
pub fn new_ipc_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::invalid(codes::SEAL_RANDOM_FAILED))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn id_arg(arg: Option<&str>) -> std::result::Result<i64, IpcError> {
    arg.and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| IpcError::new(IpcError::BAD_ARGUMENT, "expected integer id"))
}

fn to_json<T: serde::Serialize>(value: T) -> std::result::Result<String, IpcError> {
    serde_json::to_string(&value)
        .map_err(|error| IpcError::new("ipc.serialize", error.to_string()))
}

fn to_ipc(error: Error) -> IpcError {
    IpcError::new(error.code(), error.to_string())
}

fn work_json(entry: &ShelfEntry) -> Value {
    json!({
        "id": entry.work.id,
        "kind": entry.work.kind.as_str(),
        "title": &entry.work.title,
        "language": entry.work.language.as_str(),
        "target_words": entry.work.target_words,
        "summary": &entry.work.summary,
        "storyline": &entry.work.storyline,
        "genre": &entry.work.genre,
        "tags": &entry.work.tags,
        "status": entry.work.status.as_str(),
        "created_at": entry.work.created_at,
        "updated_at": entry.work.updated_at,
        "opened_at": entry.work.opened_at,
        "chapters": entry.chapters,
        "word_count": entry.word_count,
        "char_count": entry.char_count,
        "chars_no_punct": entry.chars_no_punct,
    })
}

fn node_json(node: &NodeSummary) -> Value {
    json!({
        "id": node.id,
        "work_id": node.work_id,
        "parent_id": node.parent_id,
        "kind": node.kind.as_str(),
        "title": &node.title,
        "title_rendered": &node.title_rendered,
        "sort_order": node.sort_order,
        "word_count": node.word_count,
        "char_count": node.char_count,
        "chars_no_punct": node.chars_no_punct,
        "has_body": node.has_body,
        "summary": &node.summary,
        "has_children": node.has_children,
    })
}

fn outline_json(row: &ModuleOutlineRow) -> Value {
    json!({ "node": node_json(&row.node), "lines": &row.lines })
}
