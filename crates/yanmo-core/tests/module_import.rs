//! 导入闸验收：**结构 / 协议 / 指纹 / 来源 / 授予**五件事的事实核对。
//!
//! 这里明确不测"正版"——核心不做那个判断。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::store::{inspect_module_package, Store};
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};

const ID: &str = "app.example.imported";

fn manifest_json(protocol: u32) -> String {
    json!({
        "id": ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "导入测试" },
        "description": { "zh-Hans": "一张可以导入的卡。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": protocol,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": { "read": ["works"], "own_data": true }
    })
    .to_string()
}

fn package(dir: &Path, protocol: u32) -> PathBuf {
    let module = dir.join("module");
    fs::create_dir(&module).unwrap();
    fs::write(module.join("manifest.json"), manifest_json(protocol)).unwrap();
    fs::write(module.join("module.wasm"), wat::parse_str("(module)").unwrap()).unwrap();
    module
}

fn grant(read: &[CoreDataArea]) -> ModuleCapabilities {
    ModuleCapabilities {
        read: read.to_vec(),
        own_data: true,
        ..ModuleCapabilities::none()
    }
}

#[test]
fn a_valid_package_previews_structure_protocol_and_fingerprint() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 1);
    let preview = inspect_module_package(&module).unwrap();
    assert_eq!(preview.module_id, ID);
    assert_eq!(preview.name, "导入测试");
    assert_eq!(preview.version, "1.0.0");
    assert_eq!(preview.author, "Example Studio");
    assert!(preview.compatible);
    assert_eq!(preview.expected_protocol, 1);
    assert_eq!(preview.fingerprint.len(), 64);
    assert!(preview.fingerprint.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(preview.declared.read, vec![CoreDataArea::Works]);
    assert!(preview.declared.own_data);
    assert!(preview.entry_bytes > 0);
}

#[test]
fn import_records_fingerprint_and_source() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 1);
    let preview = inspect_module_package(&module).unwrap();
    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    store
        .import_module(&module, &grant(&[CoreDataArea::Works]), &preview.fingerprint, "folder")
        .unwrap();

    let card = &store.module_cards().unwrap()[0];
    assert_eq!(card.id, ID);
    assert_eq!(card.fingerprint, preview.fingerprint);
    assert_eq!(card.source, "folder");
    assert_eq!(card.reads, vec!["works".to_string()]);

    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity = 'modules' ORDER BY seq")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(rows.iter().any(|(op, payload)| op == "import"
        && payload.contains(&preview.fingerprint)
        && payload.contains("\"source\":\"folder\"")));
}

#[test]
fn a_changed_package_is_refused() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 1);
    let preview = inspect_module_package(&module).unwrap();
    // 预览之后载荷变了（模拟 TOCTOU）
    fs::write(module.join("module.wasm"), wat::parse_str("(module (memory 1))").unwrap()).unwrap();

    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let error = store
        .import_module(&module, &grant(&[CoreDataArea::Works]), &preview.fingerprint, "folder")
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_IMPORT_CHANGED);
    assert!(store.module_cards().unwrap().is_empty(), "变了就不许装");
}

#[test]
fn an_incompatible_protocol_is_reported_and_not_installed() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 2);
    let error = inspect_module_package(&module).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PROTOCOL_INCOMPATIBLE);
    let params: std::collections::BTreeMap<_, _> = error.params().into_iter().collect();
    assert_eq!(params.get("actual").map(String::as_str), Some("2"));
    assert_eq!(params.get("expected").map(String::as_str), Some("1"));

    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let error = store
        .import_module(&module, &grant(&[CoreDataArea::Works]), "", "folder")
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PROTOCOL_INCOMPATIBLE);
    assert!(store.module_cards().unwrap().is_empty());
}

#[test]
fn native_executables_and_missing_entries_are_rejected() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 1);
    fs::write(module.join("evil.exe"), b"MZ").unwrap();
    let error = inspect_module_package(&module).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PACKAGE_INVALID);
    let detail = error
        .params()
        .into_iter()
        .find(|(name, _)| *name == "detail")
        .map(|(_, value)| value)
        .unwrap_or_default();
    assert!(detail.contains("executable"), "{detail}");

    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    fs::write(empty.join("manifest.json"), manifest_json(1)).unwrap();
    let error = inspect_module_package(&empty).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PACKAGE_INVALID);
}

#[test]
fn granting_more_than_declared_is_refused() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 1);
    let preview = inspect_module_package(&module).unwrap();
    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let error = store
        .import_module(
            &module,
            &grant(&[CoreDataArea::Works, CoreDataArea::Body]),
            &preview.fingerprint,
            "folder",
        )
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_NOT_DECLARED);
}

#[test]
fn managed_import_copies_the_package_and_verifies_it_before_launch() {
    let dir = TempDir::new().unwrap();
    let source = package(dir.path(), 1);
    let preview = inspect_module_package(&source).unwrap();
    let managed = dir.path().join("managed").join(ID);
    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    store
        .import_module_managed(
            &source,
            &managed,
            &grant(&[CoreDataArea::Works]),
            &preview.fingerprint,
            "folder",
            &yanmo_core::store::CertificationSet::empty(),
        )
        .unwrap();

    assert!(managed.join("manifest.json").is_file());
    assert!(managed.join("module.wasm").is_file());
    store.verify_module_package(ID, &managed).unwrap();

    // 受管包被改：启动前核对必须拒
    fs::write(managed.join("module.wasm"), wat::parse_str("(module (memory 1))").unwrap()).unwrap();
    let error = store.verify_module_package(ID, &managed).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PACKAGE_TAMPERED);

    // 已装过同一 id：再导入直接拒
    let error = store
        .import_module_managed(
            &source,
            &managed,
            &grant(&[CoreDataArea::Works]),
            &preview.fingerprint,
            "folder",
            &yanmo_core::store::CertificationSet::empty(),
        )
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_ALREADY_INSTALLED);
}

#[test]
fn reinstalling_replaces_the_managed_package() {
    let dir = TempDir::new().unwrap();
    let source = package(dir.path(), 1);
    let managed = dir.path().join("managed").join(ID);
    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let first = inspect_module_package(&source).unwrap();
    store
        .import_module_managed(
            &source,
            &managed,
            &grant(&[CoreDataArea::Works]),
            &first.fingerprint,
            "folder",
            &yanmo_core::store::CertificationSet::empty(),
        )
        .unwrap();
    store.uninstall_module(ID).unwrap();

    // 换一份载荷再装：受管目录要换成新包，并复核新指纹
    fs::write(source.join("module.wasm"), wat::parse_str("(module (memory 1))").unwrap()).unwrap();
    let second = inspect_module_package(&source).unwrap();
    assert_ne!(first.fingerprint, second.fingerprint);
    store
        .import_module_managed(
            &source,
            &managed,
            &grant(&[CoreDataArea::Works]),
            &second.fingerprint,
            "folder",
            &yanmo_core::store::CertificationSet::empty(),
        )
        .unwrap();
    store.verify_module_package(ID, &managed).unwrap();
    assert_eq!(store.module_cards().unwrap()[0].fingerprint, second.fingerprint);
}

/// 包里的脚本：`ui/` 下放行（壳只当网页发），包根上的仍按可执行拒。
#[test]
fn scripts_are_allowed_only_under_ui() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path(), 1);
    fs::create_dir_all(module.join("ui")).unwrap();
    fs::write(module.join("ui").join("app.js"), b"console.log(1)").unwrap();
    let preview = inspect_module_package(&module).expect("ui 下的脚本是界面的正常组成");
    assert_eq!(preview.module_id, ID);

    fs::write(module.join("bad.js"), b"console.log(1)").unwrap();
    let error = inspect_module_package(&module).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PACKAGE_INVALID);
}
