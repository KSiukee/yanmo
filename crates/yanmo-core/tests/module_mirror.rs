//! 模块数据镜像验收：**写得出来、看得出差、照着能重建、改过的镜像不认**。
//!
//! 一组七件事：开放格式可读 / 文本与二进制一字不差地回来 / 重建要先隔离 /
//! schema 版本跟着回来 / 核对三档 / 被改过的镜像在动库之前就被拒 / 自定义表也能往返。

use std::path::PathBuf;

use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::model::{ModuleColumn, ModuleColumnType, ModuleMigration, ModuleSchemaStep, ModuleTable};
use yanmo_core::store::ModuleData;

const MODULE: &str = "app.example.map";

fn fresh() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.db");
    (dir, path)
}

fn mirror_json(data: &ModuleData, module_id: &str) -> serde_json::Value {
    let text = std::fs::read(data.mirror_path(module_id)).expect("镜像应当写在磁盘上");
    serde_json::from_slice(&text).expect("镜像应当是合法 JSON")
}

#[test]
fn a_mirror_is_open_format_and_carries_one_module_only() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "theme", b"dark").unwrap();
    data.kv_put(MODULE, "grid", b"on").unwrap();
    data.kv_put("app.example.other", "k", b"v").unwrap();
    data.record_schema(MODULE, 1).unwrap();

    let report = data.mirror(MODULE).unwrap();
    assert_eq!(report.tables, 1);
    assert_eq!(report.rows, 2);
    assert!(report.bytes > 0);
    assert!(report.path.exists());
    // 没有留下半截临时文件
    let temp = PathBuf::from(format!("{}.tmp", report.path.display()));
    assert!(!temp.exists(), "原子写不该留下 .tmp");

    let json = mirror_json(&data, MODULE);
    assert_eq!(json["format"], "yanmo-module-mirror");
    assert_eq!(json["version"], 1);
    assert_eq!(json["module_id"], MODULE);
    assert_eq!(json["schema_version"], 1);
    let tables = json["tables"].as_array().unwrap();
    assert_eq!(tables.len(), 1, "别人的表一格都不许跟过来");
    assert_eq!(tables[0]["name"], "mod_app.example.map_kv");
    let columns: Vec<&str> = tables[0]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|column| column["name"].as_str().unwrap())
        .collect();
    assert_eq!(columns, vec!["key", "value", "updated_at"]);
    assert_eq!(tables[0]["rows"].as_array().unwrap().len(), 2);
}

#[test]
fn refresh_only_writes_when_the_mirror_is_out_of_date() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();

    assert!(data.refresh_mirror_if_stale(MODULE).unwrap().is_some(), "还没有镜像：应当写一份");
    assert!(data.refresh_mirror_if_stale(MODULE).unwrap().is_none(), "一致：什么都不做");

    data.kv_put(MODULE, "k", b"changed").unwrap();
    assert!(data.refresh_mirror_if_stale(MODULE).unwrap().is_some(), "变了：重写");
    assert_eq!(data.check_mirror(MODULE).unwrap().state, "ok");
}

#[test]
fn a_broken_mirror_is_replaced_instead_of_blocking_the_refresh() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    data.mirror(MODULE).unwrap();
    std::fs::write(data.mirror_path(MODULE), b"{ this is not json").unwrap();

    let written = data.refresh_mirror_if_stale(MODULE).unwrap();
    assert!(written.is_some(), "镜像坏了正是该重写的时候");
    assert_eq!(data.check_mirror(MODULE).unwrap().state, "ok");
}

#[test]
fn mirror_all_covers_every_module_that_has_a_ledger() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put("app.example.a", "k", b"1").unwrap();
    data.kv_put("app.example.b", "k", b"2").unwrap();
    data.record_schema("app.example.empty", 1).unwrap();

    let receipts = data.mirror_all().unwrap();
    let ids: Vec<&str> = receipts.iter().map(|receipt| receipt.module_id.as_str()).collect();
    assert_eq!(ids, vec!["app.example.a", "app.example.b", "app.example.empty"]);
    for receipt in &receipts {
        assert!(receipt.path.exists(), "{} 应当有镜像文件", receipt.module_id);
    }
    let empty = receipts.iter().find(|receipt| receipt.module_id == "app.example.empty").unwrap();
    assert_eq!(empty.tables, 0, "只有账、没有表");
    assert_eq!(mirror_json(&data, "app.example.empty")["schema_version"], 1);
    // 别人的表依旧不会跟过来
    assert_eq!(mirror_json(&data, "app.example.a")["tables"].as_array().unwrap().len(), 1);
}

#[test]
fn text_and_binary_survive_the_round_trip() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    let text = "一段中文 · 😀 · 行尾\n".as_bytes().to_vec();
    let binary: Vec<u8> = (0..=255u8).collect();
    data.kv_put(MODULE, "text", &text).unwrap();
    data.kv_put(MODULE, "binary", &binary).unwrap();
    data.mirror(MODULE).unwrap();

    // 模拟「这一格坏了」：绕过核心直接把表删掉
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("DROP TABLE \"mod_app.example.map_kv\"", []).unwrap();
    }
    assert!(data.kv_get(MODULE, "text").unwrap().is_none(), "先确认数据真的没了");

    data.isolate(MODULE, "damaged").unwrap();
    let rebuilt = data.rebuild_from_mirror(MODULE).unwrap();
    assert_eq!(rebuilt.tables, 1);
    assert_eq!(rebuilt.rows, 2);
    data.release_isolation(MODULE).unwrap();

    assert_eq!(data.kv_get(MODULE, "text").unwrap(), Some(text));
    assert_eq!(data.kv_get(MODULE, "binary").unwrap(), Some(binary));
}

#[test]
fn rebuild_refuses_to_overwrite_live_data_unless_isolated() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    data.mirror(MODULE).unwrap();

    let error = data.rebuild_from_mirror(MODULE).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_DATA_REBUILD_LOCKED);
    assert_eq!(data.kv_get(MODULE, "k").unwrap(), Some(b"v".to_vec()), "被拒时活数据一个字都不许动");

    data.isolate(MODULE, "manual").unwrap();
    assert!(data.rebuild_from_mirror(MODULE).is_ok());
    // 重建不会顺手解除隔离：那是作者的下一个决定
    assert!(data.isolation(MODULE).unwrap().is_some());
}

#[test]
fn an_empty_namespace_can_be_rebuilt_without_isolation() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    data.record_schema(MODULE, 2).unwrap();
    data.mirror(MODULE).unwrap();
    data.drop_namespace(MODULE).unwrap();
    assert!(data.isolation(MODULE).unwrap().is_none(), "清数据会连隔离一起收掉");

    let rebuilt = data.rebuild_from_mirror(MODULE).unwrap();
    assert_eq!(rebuilt.schema_version, Some(2));
    assert_eq!(data.schema_version(MODULE).unwrap(), Some(2));
    assert_eq!(data.kv_get(MODULE, "k").unwrap(), Some(b"v".to_vec()));
}

#[test]
fn check_reports_ok_different_and_missing() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    data.mirror(MODULE).unwrap();

    let ok = data.check_mirror(MODULE).unwrap();
    assert_eq!(ok.state, "ok");
    assert!(ok.differences.is_empty());
    assert!(ok.written_at > 0);

    data.kv_put(MODULE, "k", b"changed").unwrap();
    let different = data.check_mirror(MODULE).unwrap();
    assert_eq!(different.state, "different");
    assert_eq!(different.differences[0].code, "content");
    assert_eq!(different.differences[0].detail, "mod_app.example.map_kv");

    std::fs::remove_file(data.mirror_path(MODULE)).unwrap();
    let missing = data.check_mirror(MODULE).unwrap();
    assert_eq!(missing.state, "missing");
    assert_eq!(missing.written_at, 0);
}

#[test]
fn a_tampered_mirror_is_rejected_before_the_database_is_touched() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    data.mirror(MODULE).unwrap();
    data.isolate(MODULE, "manual").unwrap();

    // ① 把表名改成别人的命名空间
    let mut json = mirror_json(&data, MODULE);
    json["tables"][0]["name"] = serde_json::json!("mod_app.example.other_kv");
    std::fs::write(data.mirror_path(MODULE), serde_json::to_vec(&json).unwrap()).unwrap();
    let error = data.rebuild_from_mirror(MODULE).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_MIRROR_INVALID);
    assert_eq!(data.tables_of(MODULE).unwrap().len(), 1, "被拒时表还在");

    // ② 把列类型改成能拼进 SQL 注入的形状
    let mut json = mirror_json(&data, MODULE);
    json["tables"][0]["columns"][0]["type"] = serde_json::json!("TEXT); DROP TABLE module_schema; --");
    // 先恢复一个合法表名再写回
    json["tables"][0]["name"] = serde_json::json!("mod_app.example.map_kv");
    std::fs::write(data.mirror_path(MODULE), serde_json::to_vec(&json).unwrap()).unwrap();
    let error = data.rebuild_from_mirror(MODULE).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_MIRROR_INVALID);
    assert_eq!(data.kv_get(MODULE, "k").unwrap_err().code(), codes::MODULE_DATA_ISOLATED);
    assert_eq!(data.tables_of(MODULE).unwrap().len(), 1);
}

#[test]
fn custom_tables_round_trip_with_their_columns() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    let migration = ModuleMigration {
        version: 1,
        steps: vec![ModuleSchemaStep::CreateTable {
            table: ModuleTable {
                name: "places".to_string(),
                columns: vec![
                    ModuleColumn {
                        name: "id".to_string(),
                        kind: ModuleColumnType::Integer,
                        not_null: true,
                        primary_key: true,
                    },
                    ModuleColumn {
                        name: "name".to_string(),
                        kind: ModuleColumnType::Text,
                        not_null: false,
                        primary_key: false,
                    },
                ],
            },
        }],
    };
    data.apply_migration(MODULE, &migration).unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("INSERT INTO \"mod_app.example.map_places\"(id, name) VALUES (1, '北山'), (2, '渡口')", [])
            .unwrap();
    }
    let report = data.mirror(MODULE).unwrap();
    assert_eq!(report.tables, 1);
    assert_eq!(report.rows, 2);
    assert_eq!(data.check_mirror(MODULE).unwrap().state, "ok");

    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("DROP TABLE \"mod_app.example.map_places\"", []).unwrap();
    }
    data.isolate(MODULE, "damaged").unwrap();
    let rebuilt = data.rebuild_from_mirror(MODULE).unwrap();
    assert_eq!(rebuilt.rows, 2);

    let conn = rusqlite::Connection::open(&path).unwrap();
    let names: Vec<String> = conn
        .prepare("SELECT name FROM \"mod_app.example.map_places\" ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(names, vec!["北山".to_string(), "渡口".to_string()]);
    assert_eq!(data.schema_version(MODULE).unwrap(), Some(1));
}
