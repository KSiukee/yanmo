//! 常驻启动器：**控制行走 stdin → 启动器 → 宿主**，宿主能跑两次再干净退出。

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use yanmo_core::store::Store;

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
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
