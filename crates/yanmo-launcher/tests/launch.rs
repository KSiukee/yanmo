//! 启动器验收：**起服务、拉宿主、从 stdin 给 token、等退出、收报告**。
//!
//! 需要先编出 `yanmo-host`（`cargo test --workspace` 会编）；单独跑这个 crate 时
//! 找不到宿主二进制就跳过这一条，并明确打印原因。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::TempDir;
use yanmo_core::model::WorkKind;
use yanmo_core::store::Store;
use yanmo_launcher::{launch, LaunchError, LaunchRequest};
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

const ID: &str = "app.example.map";

fn host_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("YANMO_HOST_BIN") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let name = if cfg!(windows) { "yanmo-host.exe" } else { "yanmo-host" };
    let path = dir.join(name);
    path.is_file().then_some(path)
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
            "read": ["works"],
            "own_data": true,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    });
    ModuleManifest::from_json(&value.to_string()).unwrap()
}

fn grant() -> ModuleCapabilities {
    ModuleCapabilities {
        read: vec![CoreDataArea::Works],
        own_data: true,
        ..ModuleCapabilities::none()
    }
}

const READ_WAT: &str = r#"
    (module
      (import "env" "emit" (func $emit (param i32 i32) (result i32)))
      (import "env" "read" (func $read (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "run") (result i32)
        (local $n i32)
        (local.set $n
          (call $read (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 100) (i32.const 4096)))
        (if (i32.lt_s (local.get $n) (i32.const 0))
          (then (return (local.get $n))))
        (call $emit (i32.const 100) (local.get $n))))
"#;

fn package(dir: &Path) -> PathBuf {
    let module = dir.join("module");
    fs::create_dir(&module).unwrap();
    fs::write(
        module.join("manifest.json"),
        serde_json::to_string(&manifest()).unwrap(),
    )
    .unwrap();
    fs::write(module.join("module.wasm"), wat::parse_str(READ_WAT).unwrap()).unwrap();
    module
}

fn seeded_db(dir: &Path) -> PathBuf {
    let db = dir.join("yanmo.db");
    let mut store = Store::open(&db).unwrap();
    store.create_work(WorkKind::Novel, "题本").unwrap();
    store.install_module(&manifest(), &grant()).unwrap();
    drop(store);
    db
}

#[test]
fn launch_runs_the_sandbox_and_reads_a_work() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    let db_path = seeded_db(dir.path());
    let module_dir = package(dir.path());

    let report = launch(&LaunchRequest {
        db_path,
        module_dir,
        host_bin,
        timeout: Duration::from_secs(30),
        resident: false,
    })
    .unwrap();

    assert!(!report.timed_out);
    assert_eq!(report.exit_code, Some(0), "stderr: {}", report.stderr);
    assert_eq!(report.module_id, ID);
    assert!(report.ipc_error.is_none(), "ipc: {:?}", report.ipc_error);
    assert!(report.stdout.contains("题本"), "stdout: {}", report.stdout);
}

#[test]
fn a_missing_manifest_is_reported() {
    let dir = TempDir::new().unwrap();
    let error = launch(&LaunchRequest {
        db_path: seeded_db(dir.path()),
        module_dir: dir.path().join("nope"),
        host_bin: host_bin().unwrap_or_else(|| PathBuf::from("yanmo-host")),
        timeout: Duration::from_secs(5),
        resident: false,
    })
    .unwrap_err();
    assert_eq!(error.code, LaunchError::MANIFEST);
}

#[test]
fn a_missing_host_binary_is_reported_before_anything_starts() {
    let dir = TempDir::new().unwrap();
    let error = launch(&LaunchRequest {
        db_path: seeded_db(dir.path()),
        module_dir: package(dir.path()),
        host_bin: dir.path().join("no-such-host"),
        timeout: Duration::from_secs(5),
        resident: false,
    })
    .unwrap_err();
    assert_eq!(error.code, LaunchError::SPAWN);
}
