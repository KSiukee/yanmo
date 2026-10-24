//! 常驻宿主：**同一进程里跑两次**，靠 stdin 的 `run` / `stop` 控制。
//!
//! 这一条不碰核心 IPC；只证明“宿主不退出、能复用、能停”。

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::TempDir;

fn package(dir: &Path) {
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": "app.example.resident",
            "default_language": "zh-Hans",
            "name": { "zh-Hans": "常驻测试" },
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
        dir.join("module.wasm"),
        wat::parse_str(r#"(module (func (export "run") (result i32) (i32.const 0)))"#).unwrap(),
    )
    .unwrap();
}


fn named_package(root: &Path, id: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": id,
            "default_language": "zh-Hans",
            "name": { "zh-Hans": id },
            "description": { "zh-Hans": "多模块测试。" },
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

fn report(line: &str) -> Value {
    serde_json::from_str(line.trim()).expect("宿主每次运行应输出一行 JSON 报告")
}

#[test]
fn resident_host_runs_twice_and_stops_on_command() {
    let dir = TempDir::new().unwrap();
    package(dir.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args(["--module", dir.path().to_str().unwrap(), "--resident"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let mut line = String::new();
    stdin.write_all(b"run\n").unwrap();
    stdin.flush().unwrap();
    reader.read_line(&mut line).unwrap();
    let first = report(&line);
    assert_eq!(first["ok"], true);
    assert_eq!(first["status"]["kind"], "ok");

    line.clear();
    stdin.write_all(b"run\n").unwrap();
    stdin.flush().unwrap();
    reader.read_line(&mut line).unwrap();
    let second = report(&line);
    assert_eq!(second["module_id"], first["module_id"]);

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "收到 stop 后宿主应干净退出");
}

#[test]
fn resident_host_idles_out() {
    let dir = TempDir::new().unwrap();
    package(dir.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--module",
            dir.path().to_str().unwrap(),
            "--resident",
            "--idle-secs",
            "1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(b"run\n").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(report(&line)["status"]["kind"], "ok");

    let started = std::time::Instant::now();
    assert!(child.wait().unwrap().success(), "空闲超时也应干净退出");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn one_host_runs_two_modules() {
    let dir = TempDir::new().unwrap();
    named_package(dir.path(), "app.example.one");
    named_package(dir.path(), "app.example.two");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--resident",
            "--idle-secs",
            "10",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    for id in ["app.example.one", "app.example.two"] {
        let mut line = String::new();
        stdin.write_all(format!("run {id}\n").as_bytes()).unwrap();
        stdin.flush().unwrap();
        reader.read_line(&mut line).unwrap();
        assert_eq!(report(&line)["module_id"], id);
    }
    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
}
