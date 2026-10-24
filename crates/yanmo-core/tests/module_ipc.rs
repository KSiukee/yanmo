//! 核心侧 IPC 验收：**只读能力表接真库 + 门卫照旧**。
//!
//! 宿主连进来读 `works` / `tree` 能拿到真数据；没授予的 `body` 被门卫拒；
//! 冻结的模块什么都读不到；token 是 64 位十六进制。
//!
//! 传输在 `yanmo-ipc`（测试依赖）里；核心本身不含 `std::net`。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread::JoinHandle;

use serde_json::{json, Value};
use tempfile::TempDir;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{new_ipc_token, ModuleIpcHandler, Store};
use yanmo_ipc::{bind_loopback, serve_one, Client};
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::ipc::IpcError;
use yanmo_proto::manifest::ModuleManifest;

const ID: &str = "app.example.map";

fn fresh() -> (TempDir, Store, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let store = Store::open(&path).unwrap();
    (dir, store, path)
}

fn start(
    path: PathBuf,
    module_id: &str,
    token: &str,
) -> (SocketAddr, JoinHandle<Result<(), IpcError>>) {
    let listener = bind_loopback().unwrap();
    let addr = listener.local_addr().unwrap();
    let module_id = module_id.to_string();
    let token = token.to_string();
    let handle = std::thread::spawn(move || {
        let store = Store::open(path)
            .map_err(|error| IpcError::new(error.code(), error.to_string()))?;
        let mut handler = ModuleIpcHandler::new(store, module_id);
        serve_one(listener, token, &mut handler)
    });
    (addr, handle)
}

fn manifest() -> ModuleManifest {
    let value: Value = json!({
        "id": ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "剧情地图" },
        "description": { "zh-Hans": "按章节看地点怎么走。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": {
            "read": ["works", "tree", "body"],
            "own_data": true,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    });
    ModuleManifest::from_json(&value.to_string()).unwrap()
}

fn grant(read: &[CoreDataArea]) -> ModuleCapabilities {
    ModuleCapabilities { read: read.to_vec(), own_data: true, ..ModuleCapabilities::none() }
}

fn seeded(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    store.write_body(chapter, "第一章的正文。").unwrap();
    (work.id, chapter)
}

#[test]
fn a_granted_module_reads_real_data_over_ipc() {
    let (_dir, mut store, path) = fresh();
    let (work, _chapter) = seeded(&mut store);
    store
        .install_module(&manifest(), &grant(&[CoreDataArea::Works, CoreDataArea::Tree]))
        .unwrap();

    let (addr, server) = start(path, ID, "token");
    let mut client = Client::connect(addr, "token").unwrap();

    let works: Value = serde_json::from_str(&client.read(CoreDataArea::Works, None).unwrap()).unwrap();
    assert_eq!(works.as_array().unwrap().len(), 1);
    assert_eq!(works[0]["title"], "题本");
    assert_eq!(works[0]["chapters"], 1);

    let tree: Value =
        serde_json::from_str(&client.read(CoreDataArea::Tree, Some(&work.to_string())).unwrap())
            .unwrap();
    assert!(
        tree.as_array().unwrap().iter().any(|node| node["title"] == "第一章"),
        "目录树里应当有第一章：{tree}"
    );

    drop(client);
    server.join().unwrap().unwrap();
}

#[test]
fn ungranted_areas_are_denied_by_the_core_gate() {
    let (_dir, mut store, path) = fresh();
    let (_work, chapter) = seeded(&mut store);
    store.install_module(&manifest(), &grant(&[CoreDataArea::Works])).unwrap();

    let (addr, server) = start(path, ID, "token");
    let mut client = Client::connect(addr, "token").unwrap();
    let error = client
        .read(CoreDataArea::Body, Some(&chapter.to_string()))
        .unwrap_err();
    assert_eq!(error.code, "module.capability_denied", "{}", error.detail);
    drop(client);
    server.join().unwrap().unwrap();
}

#[test]
fn a_frozen_module_cannot_read_anything() {
    let (_dir, mut store, path) = fresh();
    seeded(&mut store);
    store.install_module(&manifest(), &grant(&[CoreDataArea::Works])).unwrap();
    store.set_module_enabled(ID, false).unwrap();

    let (addr, server) = start(path, ID, "token");
    let mut client = Client::connect(addr, "token").unwrap();
    let error = client.read(CoreDataArea::Works, None).unwrap_err();
    assert_eq!(error.code, "module.disabled");
    drop(client);
    server.join().unwrap().unwrap();
}

#[test]
fn an_unknown_module_is_reported_not_guessed() {
    let (_dir, mut store, path) = fresh();
    seeded(&mut store);
    let (addr, server) = start(path, "app.example.none", "token");
    let mut client = Client::connect(addr, "token").unwrap();
    let error = client.read(CoreDataArea::Works, None).unwrap_err();
    assert_eq!(error.code, "module.not_found");
    drop(client);
    server.join().unwrap().unwrap();
}

#[test]
fn tokens_are_random_hex() {
    let one = new_ipc_token().unwrap();
    let two = new_ipc_token().unwrap();
    assert_eq!(one.len(), 64);
    assert!(one.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_ne!(one, two);
}
