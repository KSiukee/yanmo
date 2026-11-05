//! 模块注册表验收：**声明 vs 授予、冻结、认证、记录损坏**。
//!
//! 这里只测核心侧的账本，不启动任何模块；读数据的门卫在另一组（`module_reads`）。

use serde_json::{json, Value};
use yanmo_core::error_codes::codes;
use yanmo_core::model::ModuleRecord;
use yanmo_core::Error;
use yanmo_core::store::Store;
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn base_json() -> Value {
    json!({
        "id": "app.example.map",
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
            "read": ["works", "tree", "body"],
            "own_data": true,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    })
}

fn manifest() -> ModuleManifest {
    ModuleManifest::from_json(&base_json().to_string()).unwrap()
}

fn caps(read: &[&str], own_data: bool) -> ModuleCapabilities {
    serde_json::from_value(json!({ "read": read, "own_data": own_data })).unwrap()
}

fn param(error: &Error, name: &str) -> String {
    error
        .params()
        .into_iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("缺少参数：{name}"))
}

fn ops(store: &Store) -> Vec<String> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op FROM op_log WHERE entity = 'modules' ORDER BY seq")
        .unwrap();
    stmt.query_map([], |row| row.get(0)).unwrap().map(|row| row.unwrap()).collect()
}

fn install(store: &mut Store) -> ModuleRecord {
    store.install_module(&manifest(), &caps(&["works", "tree", "body"], true)).unwrap();
    store.module_record("app.example.map").unwrap()
}

#[test]
fn installing_keeps_the_manifest_and_the_grant() {
    let (_dir, mut store) = fresh();
    let record = install(&mut store);
    assert_eq!(record.manifest.id, "app.example.map");
    assert_eq!(record.manifest.version, "1.0.0");
    assert!(record.enabled, "装好默认是激活的");
    assert!(!record.certified, "认证不是自封的");
    assert!(record.can_read(CoreDataArea::Works));
    assert!(record.can_read(CoreDataArea::Body));
    assert!(!record.can_read(CoreDataArea::Outline), "没授予的读不到");
    assert!(record.may_own_data());
    assert!(!record.may_ask(), "声明了但没授予：仍然不许");
    assert!(!record.may_network());
    assert_eq!(ops(&store), vec!["install"]);
}

#[test]
fn the_same_module_cannot_be_installed_twice() {
    let (_dir, mut store) = fresh();
    install(&mut store);
    let error = store
        .install_module(&manifest(), &ModuleCapabilities::none())
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_ALREADY_INSTALLED);
    assert_eq!(store.module_records().unwrap().len(), 1);
}

#[test]
fn a_grant_cannot_exceed_the_declaration() {
    let (_dir, mut store) = fresh();
    // 一份只声明了 works、且不要自己数据 / 叩问的清单
    let mut narrow = manifest();
    narrow.capabilities.read = vec![CoreDataArea::Works];
    narrow.capabilities.own_data = false;
    narrow.capabilities.ask_questions = false;

    // 声明里有 works，授予里却塞了 body
    let error = store.install_module(&narrow, &caps(&["body"], false)).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_NOT_DECLARED);
    assert_eq!(param(&error, "capability"), "read.body");

    // 没声明自己的数据，却授予了
    let error = store.install_module(&narrow, &caps(&["works"], true)).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_NOT_DECLARED);
    assert_eq!(param(&error, "capability"), "own_data");

    // 没声明叩问，却授予了
    let mut granted = ModuleCapabilities::none();
    granted.ask_questions = true;
    let error = store.install_module(&narrow, &granted).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_NOT_DECLARED);
    assert_eq!(param(&error, "capability"), "ask_questions");
    assert!(store.module_records().unwrap().is_empty(), "拒收的授予不落库");
}

#[test]
fn a_malformed_grant_is_rejected() {
    let (_dir, mut store) = fresh();
    let duplicated = ModuleCapabilities {
        read: vec![CoreDataArea::Works, CoreDataArea::Works],
        ..ModuleCapabilities::none()
    };
    let error = store.install_module(&manifest(), &duplicated).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_INVALID);
    assert!(store.module_records().unwrap().is_empty());
}

#[test]
fn a_bad_manifest_never_touches_the_table() {
    let (_dir, mut store) = fresh();
    let good = ModuleManifest::from_json(&base_json().to_string()).unwrap();
    let mut newer = good.clone();
    newer.protocol = 2;
    let error = store.install_module(&newer, &ModuleCapabilities::none()).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_MANIFEST_INVALID);
    assert_eq!(param(&error, "code"), "manifest.protocol_unsupported");
    assert!(store.module_records().unwrap().is_empty());
}

#[test]
fn freezing_stops_reads_and_can_be_undone() {
    let (_dir, mut store) = fresh();
    install(&mut store);
    store.set_module_enabled("app.example.map", false).unwrap();
    let frozen = store.module_record("app.example.map").unwrap();
    assert!(!frozen.enabled);
    assert!(!frozen.can_read(CoreDataArea::Works));
    assert!(!frozen.may_own_data());
    assert!(frozen.manifest.capabilities.read.contains(&CoreDataArea::Works), "声明还在");

    store.set_module_enabled("app.example.map", true).unwrap();
    assert!(store.module_record("app.example.map").unwrap().can_read(CoreDataArea::Works));
    assert_eq!(ops(&store), vec!["install", "freeze", "enable"]);
}

#[test]
fn certification_is_written_by_the_core_only() {
    let (_dir, mut store) = fresh();
    install(&mut store);
    assert!(!store.module_record("app.example.map").unwrap().certified);
    store.set_module_certified("app.example.map", true).unwrap();
    assert!(store.module_record("app.example.map").unwrap().certified);
    store.set_module_certified("app.example.map", false).unwrap();
    assert!(!store.module_record("app.example.map").unwrap().certified);
    assert_eq!(ops(&store), vec!["install", "certify", "uncertify"]);
}

#[test]
fn granting_again_replaces_the_previous_set() {
    let (_dir, mut store) = fresh();
    install(&mut store);
    store.grant_module("app.example.map", &caps(&["works"], false)).unwrap();
    let record = store.module_record("app.example.map").unwrap();
    assert!(record.can_read(CoreDataArea::Works));
    assert!(!record.can_read(CoreDataArea::Body), "授予是替换，不是叠加");
    assert!(!record.may_own_data());

    store.grant_module("app.example.map", &caps(&["body"], false)).unwrap();
    let record = store.module_record("app.example.map").unwrap();
    assert!(!record.can_read(CoreDataArea::Works));
    assert!(record.can_read(CoreDataArea::Body));
    assert_eq!(ops(&store), vec!["install", "grant", "grant"]);

    let error = store.grant_module("app.example.map", &caps(&["foreshadows"], false)).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_NOT_DECLARED);
}

#[test]
fn unknown_modules_are_reported() {
    let (_dir, mut store) = fresh();
    assert_eq!(
        store.module_record("app.example.none").unwrap_err().code(),
        codes::MODULE_NOT_FOUND
    );
    assert_eq!(
        store.set_module_enabled("app.example.none", false).unwrap_err().code(),
        codes::MODULE_NOT_FOUND
    );
    assert_eq!(
        store.grant_module("app.example.none", &ModuleCapabilities::none()).unwrap_err().code(),
        codes::MODULE_NOT_FOUND
    );
}

#[test]
fn a_corrupt_manifest_is_visible_instead_of_silently_skipped() {
    let (_dir, mut store) = fresh();
    install(&mut store);
    store
        .conn()
        .execute("UPDATE modules SET manifest = '{}' WHERE id = 'app.example.map'", [])
        .unwrap();
    assert_eq!(
        store.module_record("app.example.map").unwrap_err().code(),
        codes::MODULE_RECORD_CORRUPT
    );
    assert_eq!(store.module_records().unwrap_err().code(), codes::MODULE_RECORD_CORRUPT);
}

#[test]
fn a_corrupt_grant_denies_everything_instead_of_granting() {
    let (_dir, mut store) = fresh();
    install(&mut store);
    store
        .conn()
        .execute("UPDATE modules SET granted = 'not json' WHERE id = 'app.example.map'", [])
        .unwrap();
    let record = store.module_record("app.example.map").unwrap();
    assert_eq!(record.granted, ModuleCapabilities::none(), "坏授予按全拒处理");
    assert!(!record.can_read(CoreDataArea::Works));
}

#[test]
fn records_are_listed_in_install_order() {
    let (_dir, mut store) = fresh();
    let first = manifest();
    store.install_module(&first, &ModuleCapabilities::none()).unwrap();
    let mut second = first.clone();
    second.id = "app.example.second".to_string();
    second.name.insert("zh-Hans".to_string(), "第二本".to_string());
    store.install_module(&second, &ModuleCapabilities::none()).unwrap();
    let ids: Vec<String> = store
        .module_records()
        .unwrap()
        .into_iter()
        .map(|record| record.id().to_string())
        .collect();
    assert_eq!(ids, vec!["app.example.map", "app.example.second"]);
}

#[test]
fn delivering_body_is_default_denied_and_needs_its_own_grant() {
    // 模块把正文交给核心，是**单独一项**授权；默认关，冻结也不能交。
    let (_dir, mut store) = fresh();
    let mut declared = base_json();
    declared["capabilities"]["deliver_body"] = json!(true);
    let manifest = ModuleManifest::from_json(&declared.to_string()).unwrap();

    store.install_module(&manifest, &caps(&["works"], false)).unwrap();
    assert!(
        !store.module_record("app.example.map").unwrap().may_deliver_body(),
        "安装时默认不给交付正文"
    );

    let mut granted = caps(&["works"], false);
    granted.deliver_body = true;
    store.grant_module("app.example.map", &granted).unwrap();
    assert!(
        store.module_record("app.example.map").unwrap().may_deliver_body(),
        "单独勾了这一项才放行"
    );

    store.set_module_enabled("app.example.map", false).unwrap();
    assert!(
        !store.module_record("app.example.map").unwrap().may_deliver_body(),
        "冻结即不能交付"
    );
}
