//! 平台模式：一个 launcher 起一个多模块宿主，`run <id>` 分两次跑。

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
