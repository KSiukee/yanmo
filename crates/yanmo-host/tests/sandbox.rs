//! 沙箱验收：**没有 WASI、没有文件、没有网络；死循环会停；输出有界；包内没有 exe**。
//!
//! 测试里的 guest 全部用 WAT 现写；不需要预编译的 wasm 文件。

use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::thread::JoinHandle;

use tempfile::TempDir;
use yanmo_host::{
    run_package, run_package_with_core, HostError, ModulePackage, RunStatus, SandboxConfig,
};
use yanmo_proto::capability::CoreDataArea;
use yanmo_ipc::{bind_loopback, serve_one, Client};
use yanmo_proto::ipc::{IpcError, ReadHandler};

const MODULE_ID: &str = "app.example.sandbox";

fn manifest_json_with_read(reads: &[&str]) -> String {
    serde_json::json!({
        "id": MODULE_ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "沙箱测试" },
        "description": { "zh-Hans": "只用来跑沙箱验收。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": {
            "read": reads,
            "own_data": true,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    })
    .to_string()
}

fn manifest_json() -> String {
    manifest_json_with_read(&[])
}

fn package_with(dir: &Path, wat: &str) -> ModulePackage {
    package_with_read(dir, wat, &[])
}

fn package_with_read(dir: &Path, wat: &str, reads: &[&str]) -> ModulePackage {
    fs::write(dir.join("manifest.json"), manifest_json_with_read(reads)).unwrap();
    let wasm = wat::parse_str(wat).unwrap();
    fs::write(dir.join("module.wasm"), wasm).unwrap();
    ModulePackage::open(dir).unwrap()
}

fn fresh() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn config() -> SandboxConfig {
    SandboxConfig { fuel: 100_000, ..SandboxConfig::default() }
}

#[test]
fn a_valid_module_emits_and_returns_ok() {
    let dir = fresh();
    let package = package_with(
        dir.path(),
        r#"
        (module
          (import "env" "emit" (func $emit (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "hello")
          (func (export "run") (result i32)
            (call $emit (i32.const 0) (i32.const 5))))
        "#,
    );
    let report = run_package(&package, config()).unwrap();
    assert_eq!(report.module_id, MODULE_ID);
    assert_eq!(report.status, RunStatus::Ok);
    assert_eq!(report.emitted, vec!["hello".to_string()]);
    assert!(report.fuel_used > 0, "跑过就该消耗燃料");
    assert_eq!(report.memory_bytes, 65_536, "默认一页内存");
}

#[test]
fn wasi_imports_are_refused() {
    let dir = fresh();
    let package = package_with(
        dir.path(),
        r#"
        (module
          (import "wasi_snapshot_preview1" "fd_write"
            (func $fd (param i32 i32 i32 i32) (result i32)))
          (func (export "run") (result i32) (i32.const 0)))
        "#,
    );
    let error = run_package(&package, config()).unwrap_err();
    assert_eq!(error.code, HostError::IMPORT_NOT_ALLOWED);
    assert!(error.detail.contains("wasi_snapshot_preview1"), "{}", error.detail);
}

#[test]
fn file_and_network_imports_do_not_exist() {
    for (module, name) in [("env", "read_file"), ("env", "fetch")] {
        let dir = fresh();
        let wat = format!(
            r#"
            (module
              (import "{module}" "{name}" (func $f (param i32 i32) (result i32)))
              (func (export "run") (result i32) (i32.const 0)))
            "#
        );
        let package = package_with(dir.path(), &wat);
        let error = run_package(&package, config()).unwrap_err();
        assert_eq!(error.code, HostError::IMPORT_NOT_ALLOWED, "{module}::{name}");
    }
}

#[test]
fn an_infinite_loop_is_stopped_by_fuel() {
    let dir = fresh();
    let package = package_with(
        dir.path(),
        r#"
        (module
          (func (export "run") (result i32)
            (loop $forever (br $forever))
            (unreachable)))
        "#,
    );
    let error = run_package(&package, SandboxConfig { fuel: 10_000, ..config() }).unwrap_err();
    assert_eq!(error.code, HostError::FUEL);
}

#[test]
fn a_single_oversize_emit_fails_the_run() {
    let dir = fresh();
    let text = "a".repeat(200);
    let wat = format!(
        r#"
        (module
          (import "env" "emit" (func $emit (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "{text}")
          (func (export "run") (result i32)
            (call $emit (i32.const 0) (i32.const 200))))
        "#
    );
    let package = package_with(dir.path(), &wat);
    let error = run_package(&package, SandboxConfig { max_emit_bytes: 64, ..config() }).unwrap_err();
    assert_eq!(error.code, HostError::EMIT_TOO_LARGE);
}

#[test]
fn memory_growth_is_capped() {
    let dir = fresh();
    let package = package_with(
        dir.path(),
        r#"
        (module
          (memory (export "memory") 1)
          (func (export "run") (result i32)
            (drop (memory.grow (i32.const 100)))
            (i32.const 0)))
        "#,
    );
    let report = run_package(
        &package,
        SandboxConfig { max_memory_bytes: 64 * 1024, ..config() },
    )
    .unwrap();
    assert_eq!(report.status, RunStatus::Ok);
    assert_eq!(report.memory_bytes, 65_536, "超限的 grow 不许真的涨上去");
}

#[test]
fn a_missing_run_export_is_refused() {
    let dir = fresh();
    let package = package_with(
        dir.path(),
        r#"
        (module
          (func (export "other") (result i32) (i32.const 0)))
        "#,
    );
    let error = run_package(&package, config()).unwrap_err();
    assert_eq!(error.code, HostError::MISSING_RUN);
}

#[test]
fn a_guest_error_code_is_reported_not_swallowed() {
    let dir = fresh();
    let package = package_with(
        dir.path(),
        r#"
        (module
          (func (export "run") (result i32) (i32.const 7)))
        "#,
    );
    let report = run_package(&package, config()).unwrap();
    assert_eq!(report.status, RunStatus::GuestError { code: 7 });
}

#[test]
fn native_executables_are_not_allowed_in_the_package() {
    let dir = fresh();
    fs::write(dir.path().join("manifest.json"), manifest_json()).unwrap();
    fs::write(dir.path().join("module.wasm"), wat::parse_str("(module)").unwrap()).unwrap();
    fs::write(dir.path().join("evil.exe"), b"MZ").unwrap();
    let error = ModulePackage::open(dir.path()).unwrap_err();
    assert_eq!(error.code, HostError::EXECUTABLE);
}

#[test]
fn a_missing_entry_is_reported() {
    let dir = fresh();
    fs::write(dir.path().join("manifest.json"), manifest_json()).unwrap();
    let error = ModulePackage::open(dir.path()).unwrap_err();
    assert_eq!(error.code, HostError::ENTRY_MISSING);
}

#[test]
fn invalid_wasm_is_reported() {
    let dir = fresh();
    fs::write(dir.path().join("manifest.json"), manifest_json()).unwrap();
    fs::write(dir.path().join("module.wasm"), b"this is not wasm").unwrap();
    let package = ModulePackage::open(dir.path()).unwrap();
    let error = run_package(&package, config()).unwrap_err();
    assert_eq!(error.code, HostError::WASM_INVALID);
}

#[test]
fn a_bad_manifest_is_reported() {
    let dir = fresh();
    fs::write(dir.path().join("manifest.json"), r#"{"id":"not-a-reverse-domain"}"#).unwrap();
    let error = ModulePackage::open(dir.path()).unwrap_err();
    assert_eq!(error.code, HostError::MANIFEST);
}

#[test]
fn the_independent_binary_runs_a_module_and_prints_json() {
    let dir = fresh();
    let wat = r#"
        (module
          (import "env" "emit" (func $emit (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "hi")
          (func (export "run") (result i32)
            (call $emit (i32.const 0) (i32.const 2))))
        "#;
    fs::write(dir.path().join("manifest.json"), manifest_json()).unwrap();
    fs::write(dir.path().join("module.wasm"), wat::parse_str(wat).unwrap()).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yanmo-host"))
        .args(["--module", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"module_id\""));
    assert!(stdout.contains("\"hi\""));
}

struct MockCore;

impl ReadHandler for MockCore {
    fn read(&mut self, area: CoreDataArea, arg: Option<&str>) -> Result<String, IpcError> {
        match area {
            CoreDataArea::Works => Ok(r#"[{"id":1,"title":"题本"}]"#.to_string()),
            CoreDataArea::Tree => Ok(format!(r#"{{"work":{}}}"#, arg.unwrap_or("0"))),
            _ => Err(IpcError::new("ipc.capability_unavailable", area.as_str())),
        }
    }
}

fn start_core(token: &str) -> (SocketAddr, JoinHandle<Result<(), IpcError>>) {
    let listener = bind_loopback().unwrap();
    let addr = listener.local_addr().unwrap();
    let token = token.to_string();
    let handle = std::thread::spawn(move || {
        let mut handler = MockCore;
        serve_one(listener, token, &mut handler)
    });
    (addr, handle)
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

#[test]
fn a_declared_read_is_served_from_the_core() {
    let (addr, server) = start_core("token");
    let client = Client::connect(addr, "token").unwrap();
    let dir = fresh();
    let package = package_with_read(dir.path(), READ_WAT, &["works"]);

    let report = run_package_with_core(&package, config(), Some(client)).unwrap();
    assert_eq!(report.status, RunStatus::Ok);
    assert_eq!(report.emitted, vec![r#"[{"id":1,"title":"题本"}]"#.to_string()]);
    server.join().unwrap().unwrap();
}

#[test]
fn read_without_a_declaration_is_not_linked() {
    let (addr, server) = start_core("token");
    let client = Client::connect(addr, "token").unwrap();
    let dir = fresh();
    let package = package_with(dir.path(), READ_WAT);

    let error = run_package_with_core(&package, config(), Some(client)).unwrap_err();
    assert_eq!(error.code, HostError::IMPORT_NOT_ALLOWED);
    server.join().unwrap().unwrap();
}

#[test]
fn read_without_a_core_link_is_not_linked() {
    let dir = fresh();
    let package = package_with_read(dir.path(), READ_WAT, &["works"]);
    let error = run_package(&package, config()).unwrap_err();
    assert_eq!(error.code, HostError::IMPORT_NOT_ALLOWED);
}

#[test]
fn undeclared_areas_are_denied_by_the_host() {
    let (addr, server) = start_core("token");
    let client = Client::connect(addr, "token").unwrap();
    let dir = fresh();
    let package = package_with_read(dir.path(), READ_WAT, &["tree"]);

    let error = run_package_with_core(&package, config(), Some(client)).unwrap_err();
    assert_eq!(error.code, HostError::CAPABILITY_DENIED);
    server.join().unwrap().unwrap();
}
