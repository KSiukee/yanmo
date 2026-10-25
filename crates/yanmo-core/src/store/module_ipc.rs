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

use super::module_read_log::{ReadLog, ReadLogEntry};
use super::{ModuleOutlineRow, NodeSummary, ShelfEntry, Store};
use crate::error::{codes, Error, Result};

/// 一次成功读取：给模块的 JSON + 记账用的两个口径。
struct Served {
    json: String,
    /// 有意义的字数（正文＝正文字数；其它区域 0，体积看 `json.len()`）。
    chars: u64,
    /// 这次读落在哪本书上（定位不到就是 `None`）。
    work: Option<i64>,
}

/// 把一个真实 `Store` 的只读视图接在 IPC 上。
pub struct ModuleIpcHandler {
    store: Store,
    module_id: String,
    /// 读账（可选：命令行/测试里不接也能跑，只是不留痕）。
    log: Option<ReadLog>,
}

impl ModuleIpcHandler {
    pub fn new(store: Store, module_id: impl Into<String>) -> Self {
        Self { store, module_id: module_id.into(), log: None }
    }

    /// 接上读账（启动器按"数据目录/read-audit.jsonl"建一个）。
    pub fn with_read_log(mut self, log: ReadLog) -> Self {
        self.log = Some(log);
        self
    }

    /// 这次请求大概落在哪本书上（正文的参数是节点 id，要换算成书）。
    ///
    /// 只用于记账：算不出来就是 `None`，**不影响读本身**。
    fn work_hint(&self, area: CoreDataArea, arg: Option<&str>) -> Option<i64> {
        let id = arg.and_then(|value| value.parse::<i64>().ok())?;
        match area {
            CoreDataArea::Works => None,
            CoreDataArea::Body => self.store.node_work(id).ok(),
            _ => Some(id),
        }
    }

    fn serve(&self, area: CoreDataArea, arg: Option<&str>) -> std::result::Result<Served, IpcError> {
        let reader = self.store.module_reader(&self.module_id).map_err(to_ipc)?;
        Ok(match area {
            CoreDataArea::Works => {
                let entries = reader.works().map_err(to_ipc)?;
                let json = to_json(entries.iter().map(work_json).collect::<Vec<_>>())?;
                Served { json, chars: 0, work: None }
            }
            CoreDataArea::Tree => {
                let work_id = id_arg(arg)?;
                let nodes = reader.tree(work_id).map_err(to_ipc)?;
                let json = to_json(nodes.iter().map(node_json).collect::<Vec<_>>())?;
                Served { json, chars: 0, work: Some(work_id) }
            }
            CoreDataArea::Body => {
                let node_id = id_arg(arg)?;
                let body = reader.body(node_id).map_err(to_ipc)?;
                let chars = body.chars().count() as u64;
                let work = self.store.node_work(node_id).ok();
                Served { json: to_json(body)?, chars, work }
            }
            CoreDataArea::Outline => {
                let work_id = id_arg(arg)?;
                let rows = reader.outline(work_id).map_err(to_ipc)?;
                let json = to_json(rows.iter().map(outline_json).collect::<Vec<_>>())?;
                Served { json, chars: 0, work: Some(work_id) }
            }
            CoreDataArea::Fragments => {
                let work_id = id_arg(arg)?;
                let fragments = reader.fragments(work_id).map_err(to_ipc)?;
                let json = to_json(fragments)?;
                Served { json, chars: 0, work: Some(work_id) }
            }
            CoreDataArea::Entities => {
                let work_id = id_arg(arg)?;
                let cards = reader.entities(work_id).map_err(to_ipc)?;
                let json = to_json(cards)?;
                Served { json, chars: 0, work: Some(work_id) }
            }
            CoreDataArea::Scenes => {
                let work_id = id_arg(arg)?;
                let scenes = reader.scenes(work_id).map_err(to_ipc)?;
                let json = to_json(
                    scenes
                        .iter()
                        .map(|(node_id, title, fields)| {
                            json!({ "node_id": node_id, "title": title, "fields": fields })
                        })
                        .collect::<Vec<Value>>(),
                )?;
                Served { json, chars: 0, work: Some(work_id) }
            }
            CoreDataArea::Foreshadows => {
                let work_id = id_arg(arg)?;
                let rows = reader.foreshadows(work_id).map_err(to_ipc)?;
                let json = to_json(rows)?;
                Served { json, chars: 0, work: Some(work_id) }
            }
        })
    }
}

impl ReadHandler for ModuleIpcHandler {
    fn read(&mut self, area: CoreDataArea, arg: Option<&str>) -> std::result::Result<String, IpcError> {
        let hint = self.work_hint(area, arg);
        let served = self.serve(area, arg);
        if let Some(log) = &self.log {
            let entry = match &served {
                Ok(served) => ReadLogEntry::ok(
                    &self.module_id,
                    area.as_str(),
                    served.work.or(hint),
                    served.chars,
                    served.json.len() as u64,
                ),
                Err(error) => {
                    ReadLogEntry::denied(&self.module_id, area.as_str(), hint, &error.code)
                }
            };
            // 记账是旁路：写不进去（磁盘满 / 没权限）只丢这一行，**绝不因此挡住读**。
            let _ = log.record(&entry);
        }
        served.map(|served| served.json)
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
