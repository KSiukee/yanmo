//! 联网第一刀：**只有显式允许 + 清单声明，才有 `env.http_get`；私网地址拒绝**。

use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use tempfile::TempDir;

fn package(dir: &Path) {
    fs::write(
        dir.join("manifest.json"),
        json!({
            "id": "app.example.network",
            "default_language": "zh-Hans",
            "name": { "zh-Hans": "联网测试" },
            "description": { "zh-Hans": "私网地址必须拒。" },
            "author": { "name": "Example Studio" },
            "version": "1.0.0",
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": [], "network": true }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("module.wasm"),
        wat::parse_str(
            r#"
            (module
              (import "env" "http_get" (func $get (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "http://127.0.0.1/")
              (func (export "run") (result i32)
                (call $get (i32.const 0) (i32.const 17) (i32.const 100) (i32.const 4096))))
            "#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn loopback_is_denied_by_the_host_proxy() {
    let dir = TempDir::new().unwrap();
    package(dir.path());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--module",
            dir.path().to_str().unwrap(),
            "--allow-network",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"]["kind"], "guest_error");
    assert_eq!(report["status"]["code"], -1);
}
