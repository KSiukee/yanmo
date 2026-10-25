//! 平台模式：一个 launcher 起一个多模块宿主，`run <id>` 分两次跑。

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use yanmo_core::store::Store;
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

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

fn package(root: &Path, id: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": id,
            "default_language": "zh-Hans",
            "name": { "zh-Hans": id },
            "description": { "zh-Hans": "平台测试。" },
            "author": { "name": "Example Studio" },
            "version": "1.0.0",
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": [] }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("module.wasm"),
        wat::parse_str(r#"(module (func (export "run") (result i32) (i32.const 0)))"#).unwrap(),
    )
    .unwrap();
}

#[test]
fn platform_launcher_runs_two_modules() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    package(dir.path(), "app.example.platform.one");
    package(dir.path(), "app.example.platform.two");
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "30",
            "--idle-secs",
            "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    for id in ["app.example.platform.one", "app.example.platform.two"] {
        let mut line = String::new();
        stdin.write_all(format!("run {id}\n").as_bytes()).unwrap();
        stdin.flush().unwrap();
        reader.read_line(&mut line).unwrap();
        let report: Value = serde_json::from_str(line.trim()).expect("每次运行一行报告");
        assert_eq!(report["module_id"], id);
        assert_eq!(report["status"]["kind"], "ok");
    }

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
}

fn read_package(root: &Path, id: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": id,
            "default_language": "zh-Hans",
            "name": { "zh-Hans": id },
            "description": { "zh-Hans": "模块身份路由测试。" },
            "author": { "name": "Example Studio" },
            "version": "1.0.0",
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": ["works"] }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("module.wasm"),
        wat::parse_str(r#"
            (module
              (import "env" "read" (func $read (param i32 i32 i32 i32 i32) (result i32)))
              (import "env" "emit" (func $emit (param i32 i32) (result i32)))
              (memory (export "memory") 1)
              (func (export "run") (result i32)
                (local $n i32)
                (local.set $n
                  (call $read (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 100) (i32.const 4096)))
                (if (i32.lt_s (local.get $n) (i32.const 0))
                  (then (return (local.get $n))))
                (call $emit (i32.const 100) (local.get $n))))
        "#).unwrap(),
    )
    .unwrap();
}

#[test]
fn platform_launcher_routes_module_identity() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    let id = "app.example.platform.read";
    read_package(dir.path(), id);
    let manifest_text = fs::read_to_string(dir.path().join(id).join("manifest.json")).unwrap();
    let manifest = ModuleManifest::from_json(&manifest_text).unwrap();
    let grant = ModuleCapabilities {
        read: vec![CoreDataArea::Works],
        ..ModuleCapabilities::none()
    };
    let db = dir.path().join("yanmo.db");
    let mut store = Store::open(&db).unwrap();
    store.install_module(&manifest, &grant).unwrap();
    drop(store);

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "30",
            "--idle-secs",
            "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(format!("run {id}\n").as_bytes()).unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let report: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(report["module_id"], id);
    assert_eq!(report["status"]["kind"], "ok", "{report}");

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
}
