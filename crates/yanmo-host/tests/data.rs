//! `env.data_*` 验收：own_data 授予后只读写自己的 KV；未授予时链接器里没有。

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use yanmo_host::{run_package_with_data, HostError, ModuleData, ModulePackage, RunStatus, SandboxConfig};

const MODULE_ID: &str = "app.example.data";

fn package(dir: &Path, wat: &str, own_data: bool) -> ModulePackage {
    let manifest = serde_json::json!({
        "id": MODULE_ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "数据测试" },
        "description": { "zh-Hans": "只用来跑数据 ABI 验收。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": {
            "read": [],
            "own_data": own_data,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    });
    fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    fs::write(dir.join("module.wasm"), wat::parse_str(wat).unwrap()).unwrap();
    ModulePackage::open(dir).unwrap()
}

const WAT: &str = r#"
(module
  (import "env" "data_put" (func $put (param i32 i32 i32 i32) (result i32)))
  (import "env" "data_get" (func $get (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "k")
  (data (i32.const 8) "hello")
  (func (export "run") (result i32)
    (if (i32.ne (call $put (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 5)) (i32.const 0))
      (then (return (i32.const 1))))
    (if (i32.ne (call $get (i32.const 0) (i32.const 1) (i32.const 32) (i32.const 16)) (i32.const 5))
      (then (return (i32.const 2))))
    (i32.const 0)))
"#;

fn fresh() -> TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn own_data_put_and_get_round_trip() {
    let dir = fresh();
    let package = package(dir.path(), WAT, true);
    let db = dir.path().join("modules.db");
    let data = Arc::new(Mutex::new(ModuleData::open(&db).unwrap()));
    let config = SandboxConfig { allow_own_data: true, ..SandboxConfig::default() };
    let report = run_package_with_data(&package, config, data.clone()).unwrap();
    assert_eq!(report.status, RunStatus::Ok);
    assert_eq!(
        data.lock().unwrap().kv_get(MODULE_ID, "k").unwrap(),
        Some(b"hello".to_vec())
    );
}

#[test]
fn data_imports_are_rejected_without_the_grant() {
    let dir = fresh();
    let package = package(dir.path(), WAT, true);
    let db = dir.path().join("modules.db");
    let data = Arc::new(Mutex::new(ModuleData::open(&db).unwrap()));
    let config = SandboxConfig { allow_own_data: false, ..SandboxConfig::default() };
    let error = run_package_with_data(&package, config, data).unwrap_err();
    assert_eq!(error.code, HostError::IMPORT_NOT_ALLOWED);
}
