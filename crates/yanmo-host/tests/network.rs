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
    let log = dir.path().join("network-audit.jsonl");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--module",
            dir.path().to_str().unwrap(),
            "--allow-network",
            "--network-log",
            log.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"]["kind"], "guest_error");
    assert_eq!(report["status"]["code"], -1);
    let audit = fs::read_to_string(&log).unwrap();
    assert!(audit.contains("\"module\":\"app.example.network\""), "{audit}");
    assert!(audit.contains("\"result\":\"host.network_denied\""), "{audit}");
    assert!(!audit.contains("127.0.0.1/"), "审计不记完整 URL");
}

#[test]
fn unbound_credential_name_is_rejected_before_keyring() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("manifest.json"),
        json!({
            "id": "app.example.credential",
            "default_language": "zh-Hans",
            "name": { "zh-Hans": "凭据测试" },
            "description": { "zh-Hans": "没绑定就不给。" },
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
        dir.path().join("module.wasm"),
        wat::parse_str(
            r#"
            (module
              (import "env" "http_get_auth" (func $get (param i32 i32 i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "https://example.com/")
              (data (i32.const 40) "other")
              (func (export "run") (result i32)
                (call $get (i32.const 0) (i32.const 20) (i32.const 40) (i32.const 5) (i32.const 100) (i32.const 4096))))
            "#,
        )
        .unwrap(),
    )
    .unwrap();
    let credentials = json!({
        "app.example.credential": [{ "name": "bound", "host": "example.com" }]
    })
    .to_string();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--module",
            dir.path().to_str().unwrap(),
            "--module-id",
            "app.example.credential",
            "--allow-network",
            "--credentials-json",
            &credentials,
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"]["kind"], "guest_error");
    assert_eq!(report["status"]["code"], -1);
}

#[test]
#[ignore = "需要公网：验证真实 keyring + TLS + Authorization 注入"]
fn real_authenticated_fetch() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("manifest.json"),
        json!({
            "id": "app.example.realauth",
            "default_language": "zh-Hans",
            "name": { "zh-Hans": "真机联网" },
            "description": { "zh-Hans": "跑 httpbin。" },
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
        dir.path().join("module.wasm"),
        wat::parse_str(
            r#"
            (module
              (import "env" "http_get_auth" (func $get (param i32 i32 i32 i32 i32 i32) (result i32)))
              (import "env" "emit" (func $emit (param i32 i32) (result i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "https://httpbin.org/bearer")
              (data (i32.const 40) "bearer")
              (func (export "run") (result i32)
                (local $n i32)
                (local.set $n
                  (call $get (i32.const 0) (i32.const 26) (i32.const 40) (i32.const 6) (i32.const 100) (i32.const 8192)))
                (if (i32.lt_s (local.get $n) (i32.const 1))
                  (then (return (i32.const -1))))
                (drop (call $emit (i32.const 100) (local.get $n)))
                (i32.const 0)))
            "#,
        )
        .unwrap(),
    )
    .unwrap();
    let module = "app.example.realauth";
    let log = dir.path().join("network-audit.jsonl");
    yanmo_vault::store(module, "bearer", "test-token").unwrap();
    let credentials = json!({
        module: [{ "name": "bearer", "host": "httpbin.org" }]
    })
    .to_string();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--module",
            dir.path().to_str().unwrap(),
            "--module-id",
            module,
            "--allow-network",
            "--credentials-json",
            &credentials,
            "--network-allowlist",
            "httpbin.org",
            "--network-log",
            log.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let _ = yanmo_vault::remove(module, "bearer");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let audit_text = fs::read_to_string(&log).unwrap_or_default();
    assert_eq!(
        report["status"]["kind"],
        "ok",
        "{report}\nstderr={}\naudit={audit_text}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: String = report["emitted"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(body.contains("authenticated"), "{body}");
    let audit = fs::read_to_string(log).unwrap();
    assert!(audit.contains("\"result\":\"ok\""), "{audit}");
    assert!(audit.contains("\"credential\":\"bearer\""), "{audit}");
}

#[test]
fn allowlist_denies_before_any_request() {
    let dir = TempDir::new().unwrap();
    package(dir.path());
    let log = dir.path().join("network-audit.jsonl");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args([
            "--module",
            dir.path().to_str().unwrap(),
            "--allow-network",
            "--network-allowlist",
            "other.example",
            "--network-log",
            log.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"]["kind"], "guest_error");
    assert_eq!(report["status"]["code"], -1);
    let audit = fs::read_to_string(log).unwrap();
    assert!(audit.contains("\"result\":\"host.network_denied\""), "{audit}");
}
