//! 常驻启动器：**控制行走 stdin → 启动器 → 宿主**，宿主能跑两次再干净退出。

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

fn package(dir: &Path) -> PathBuf {
    let module = dir.join("module");
    fs::create_dir(&module).unwrap();
    fs::write(
        module.join("manifest.json"),
        serde_json::json!({
            "id": "app.example.resident-launch",
            "default_language": "zh-Hans",
            "name": { "zh-Hans": "常驻启动测试" },
            "description": { "zh-Hans": "跑两次。" },
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
        module.join("module.wasm"),
        wat::parse_str(r#"(module (func (export "run") (result i32) (i32.const 0)))"#).unwrap(),
    )
    .unwrap();
    module
}

#[test]
fn resident_launcher_forwards_run_and_stop() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();
    let module = package(dir.path());

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--module",
            module.to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "30",
            "--resident",
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

    for expected in 0..2 {
        let mut line = String::new();
        stdin.write_all(b"run\n").unwrap();
        stdin.flush().unwrap();
        reader.read_line(&mut line).unwrap();
        let report: Value = serde_json::from_str(line.trim()).expect("每次运行一行报告");
        assert_eq!(report["module_id"], "app.example.resident-launch");
        assert_eq!(report["status"]["kind"], "ok", "第 {expected} 次运行");
    }

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "stop 后启动器应以 0 退出");
}

const READ_64_WAT: &str = r#"
    (module
      (import "env" "read" (func $read (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "run") (result i32)
        (local $i i32)
        (loop $L
          (drop (call $read (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0)))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br_if $L (i32.lt_u (local.get $i) (i32.const 64))))
        (i32.const 0)))
"#;

#[test]
fn resident_launcher_reconnects_after_request_limit() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    let module = dir.path().join("module");
    fs::create_dir(&module).unwrap();
    let manifest_value = serde_json::json!({
        "id": "app.example.reconnect",
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "重连测试" },
        "description": { "zh-Hans": "跑五次。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": { "read": ["works"] }
    });
    fs::write(module.join("manifest.json"), manifest_value.to_string()).unwrap();
    fs::write(module.join("module.wasm"), wat::parse_str(READ_64_WAT).unwrap()).unwrap();
    let manifest = ModuleManifest::from_json(&manifest_value.to_string()).unwrap();
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
            "--module",
            module.to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "60",
            "--resident",
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

    // 4 次 × 64 = 256 请求，正好一条连接的上限；第 5 次必须重连才能过。
    for round in 0..5 {
        let mut line = String::new();
        stdin.write_all(b"run\n").unwrap();
        stdin.flush().unwrap();
        reader.read_line(&mut line).unwrap();
        let report: Value = serde_json::from_str(line.trim()).expect("每次运行一行报告");
        assert_eq!(
            report["status"]["kind"], "ok",
            "第 {round} 次运行应成功（跨过 256 请求上限）：{report}"
        );
    }

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
}
