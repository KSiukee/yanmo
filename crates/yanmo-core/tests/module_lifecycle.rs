//! 模块生命周期验收：**默认激活、冻结即停、只认用户主动启动、卸载保留数据**。
//!
//! 宿主还没做，所以这里测的是**许可与账本**：每次启动前要一张许可、冻结/链式拒绝都留痕、
//! 卸载只动注册表不动数据。进程级"干净退出"在宿主那一层（见 docs/101）。

use serde_json::json;
use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::model::{
    ModuleColumn, ModuleColumnType, ModuleLaunchSource, ModuleMigration, ModuleSchemaStep,
    ModuleState, ModuleTable,
};
use yanmo_core::store::{ModuleData, Store};
use yanmo_core::Error;
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::manifest::ModuleManifest;

const A: &str = "app.example.a";
const B: &str = "app.example.b";

fn fresh() -> (TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn manifest(id: &str) -> ModuleManifest {
    let value = json!({
        "id": id,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "题本" },
        "description": { "zh-Hans": "生命周期测试。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": {}
    });
    ModuleManifest::from_json(&value.to_string()).unwrap()
}

fn install(store: &mut Store, id: &str) {
    store.install_module(&manifest(id), &ModuleCapabilities::none()).unwrap();
}

fn param(error: &Error, name: &str) -> String {
    error
        .params()
        .into_iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("缺少参数：{name}"))
}

fn module_ops(store: &Store) -> Vec<(String, serde_json::Value)> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity = 'modules' ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .unwrap();
    rows.map(|row| {
        let (op, payload) = row.unwrap();
        (op, serde_json::from_str(&payload).unwrap())
    })
    .collect()
}

fn data(dir: &TempDir) -> ModuleData {
    ModuleData::open(dir.path().join("modules.db")).unwrap()
}

fn create_table(data: &mut ModuleData, id: &str, suffix: &str) {
    data.apply_migration(
        id,
        &ModuleMigration {
            version: 1,
            steps: vec![ModuleSchemaStep::CreateTable {
                table: ModuleTable {
                    name: suffix.to_string(),
                    columns: vec![ModuleColumn {
                        name: "id".to_string(),
                        kind: ModuleColumnType::Integer,
                        not_null: true,
                        primary_key: true,
                    }],
                },
            }],
        },
    )
    .unwrap();
}

#[test]
fn installing_starts_active_and_freeze_is_reversible() {
    let (_dir, mut store) = fresh();
    install(&mut store, A);
    let record = store.module_record(A).unwrap();
    assert_eq!(record.state(), ModuleState::Active, "装了默认就是激活");
    assert!(record.is_active());

    store.set_module_enabled(A, false).unwrap();
    let frozen = store.module_record(A).unwrap();
    assert_eq!(frozen.state(), ModuleState::Frozen);
    assert!(!frozen.is_active());
    assert!(!frozen.can_read(yanmo_proto::capability::CoreDataArea::Works));

    store.set_module_enabled(A, true).unwrap();
    assert_eq!(store.module_record(A).unwrap().state(), ModuleState::Active);
}

#[test]
fn a_frozen_module_cannot_be_launched() {
    let (_dir, mut store) = fresh();
    install(&mut store, A);
    store.set_module_enabled(A, false).unwrap();

    let error = store
        .authorize_module_launch(A, &ModuleLaunchSource::UserOpen)
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_DISABLED);
    assert_eq!(param(&error, "module_id"), A);

    let rejected: Vec<_> = module_ops(&store)
        .into_iter()
        .filter(|(op, _)| op == "launch_rejected")
        .collect();
    assert_eq!(rejected.len(), 1, "拒绝也要留痕");
    assert_eq!(rejected[0].1["reason"], "frozen");
    assert_eq!(rejected[0].1["source"], "user_open");
}

#[test]
fn user_initiated_launches_are_permitted_and_recorded() {
    let (_dir, mut store) = fresh();
    install(&mut store, A);

    let open = store
        .authorize_module_launch(A, &ModuleLaunchSource::UserOpen)
        .unwrap();
    assert_eq!(open.module_id, A);
    assert!(open.granted_at > 0);
    assert_eq!(open.source.as_str(), "user_open");

    let action = store
        .authorize_module_launch(
            A,
            &ModuleLaunchSource::UserAction { action: "export".to_string() },
        )
        .unwrap();
    assert_eq!(action.source.as_str(), "user_action");

    let launches: Vec<_> = module_ops(&store)
        .into_iter()
        .filter(|(op, _)| op == "launch")
        .collect();
    assert_eq!(launches.len(), 2);
    assert_eq!(launches[0].1["source"], "user_open");
    assert_eq!(launches[1].1["source"], "user_action");
    assert_eq!(launches[1].1["action"], "export");
}

#[test]
fn a_chain_launch_is_refused_until_the_chain_layer_exists() {
    let (_dir, mut store) = fresh();
    install(&mut store, A);
    install(&mut store, B);

    let error = store
        .authorize_module_launch(B, &ModuleLaunchSource::ModuleChain { from: A.to_string() })
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_CHAIN_UNAVAILABLE);
    assert!(store.module_record(B).unwrap().is_active(), "目标模块没被牵连");

    let rejected: Vec<_> = module_ops(&store)
        .into_iter()
        .filter(|(op, _)| op == "launch_rejected")
        .collect();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].1["reason"], "chain_unavailable");
    assert_eq!(rejected[0].1["from"], A);
}

#[test]
fn a_chain_from_a_frozen_caller_is_refused() {
    let (_dir, mut store) = fresh();
    install(&mut store, A);
    install(&mut store, B);
    store.set_module_enabled(A, false).unwrap();

    let error = store
        .authorize_module_launch(B, &ModuleLaunchSource::ModuleChain { from: A.to_string() })
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_DISABLED);
    assert_eq!(param(&error, "module_id"), A, "要说清是发起方冻结了");
}

#[test]
fn invalid_launch_sources_are_refused() {
    let (_dir, mut store) = fresh();
    install(&mut store, A);
    for source in [
        ModuleLaunchSource::UserAction { action: String::new() },
        ModuleLaunchSource::UserAction { action: "x".repeat(65) },
        ModuleLaunchSource::UserAction { action: "a\nb".to_string() },
        ModuleLaunchSource::ModuleChain { from: "not a module".to_string() },
    ] {
        let error = store.authorize_module_launch(A, &source).unwrap_err();
        assert_eq!(error.code(), codes::MODULE_LAUNCH_SOURCE_INVALID, "source: {source:?}");
    }
}

#[test]
fn uninstall_removes_the_registry_but_keeps_the_module_data() {
    let (dir, mut store) = fresh();
    install(&mut store, A);
    let mut data = data(&dir);
    create_table(&mut data, A, "places");
    let conn = rusqlite::Connection::open(data.path()).unwrap();
    conn.execute("INSERT INTO \"mod_app.example.a_places\"(id) VALUES (1)", []).unwrap();
    drop(conn);

    store.uninstall_module(A).unwrap();
    assert_eq!(
        store.module_record(A).unwrap_err().code(),
        codes::MODULE_NOT_FOUND,
        "注册表里没有它了"
    );
    assert_eq!(
        store
            .authorize_module_launch(A, &ModuleLaunchSource::UserOpen)
            .unwrap_err()
            .code(),
        codes::MODULE_NOT_FOUND
    );
    assert_eq!(data.schema_version(A).unwrap(), Some(1), "数据版本账还在");
    assert_eq!(data.tables_of(A).unwrap(), vec!["mod_app.example.a_places".to_string()]);

    let uninstall = module_ops(&store)
        .into_iter()
        .find(|(op, _)| op == "uninstall")
        .expect("卸载要留痕");
    assert_eq!(uninstall.1["data_kept"], true);
}

#[test]
fn dropping_the_namespace_is_explicit_and_scoped() {
    let (dir, mut store) = fresh();
    install(&mut store, A);
    install(&mut store, B);
    let mut data = data(&dir);
    create_table(&mut data, A, "places");
    create_table(&mut data, B, "places");

    assert_eq!(data.drop_namespace(A).unwrap(), 1);
    assert!(data.tables_of(A).unwrap().is_empty());
    assert_eq!(data.schema_version(A).unwrap(), None);
    assert_eq!(data.tables_of(B).unwrap(), vec!["mod_app.example.b_places".to_string()]);
    assert_eq!(data.schema_version(B).unwrap(), Some(1), "别的模块不受牵连");
    assert!(store.module_record(A).is_ok(), "删数据不是卸载，注册表还在");
}

#[test]
fn uninstalling_an_unknown_module_is_reported() {
    let (_dir, mut store) = fresh();
    assert_eq!(
        store.uninstall_module("app.example.none").unwrap_err().code(),
        codes::MODULE_NOT_FOUND
    );
}
