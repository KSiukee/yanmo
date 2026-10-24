//! 模块数据区验收：**schema 版本账 + 命名空间识别**。
//!
//! 不测模块表的语义（那是模块自己的迁移），只测核心承诺的两件事：
//! 每个模块的 schema 版本记得住、每张表认得出是谁的。

use std::path::PathBuf;

use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::store::ModuleData;

fn fresh() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.db");
    (dir, path)
}

#[test]
fn schema_versions_are_recorded_per_module_and_survive_reopen() {
    let (_dir, path) = fresh();
    {
        let mut data = ModuleData::open(&path).unwrap();
        assert_eq!(data.schema_version("app.example.map").unwrap(), None);
        data.record_schema("app.example.map", 1).unwrap();
        data.record_schema("app.example.other", 3).unwrap();
        assert_eq!(data.schema_version("app.example.map").unwrap(), Some(1));
        assert_eq!(data.schema_version("app.example.other").unwrap(), Some(3));

        data.record_schema("app.example.map", 2).unwrap();
        assert_eq!(data.schema_version("app.example.map").unwrap(), Some(2));
    }
    let data = ModuleData::open(&path).unwrap();
    assert_eq!(data.schema_version("app.example.map").unwrap(), Some(2));
    assert_eq!(data.schema_version("app.example.other").unwrap(), Some(3));
}

#[test]
fn bad_ids_and_negative_versions_are_rejected() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    for bad in ["map", "App.Example", "app/map", ""] {
        assert_eq!(
            data.record_schema(bad, 1).unwrap_err().code(),
            codes::MODULE_ID_INVALID,
            "id: {bad:?}"
        );
        assert_eq!(data.schema_version(bad).unwrap_err().code(), codes::MODULE_ID_INVALID);
        assert_eq!(data.tables_of(bad).unwrap_err().code(), codes::MODULE_ID_INVALID);
    }
    assert_eq!(
        data.record_schema("app.example.map", -1).unwrap_err().code(),
        codes::MODULE_SCHEMA_INVALID
    );
}

#[test]
fn tables_are_recognized_by_the_module_prefix_only() {
    let (_dir, path) = fresh();
    // 用另一条连接造几张表（模块自己的迁移将来由宿主在受控接口里跑）
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE "mod_app.example.map_places"(id INTEGER PRIMARY KEY);
            CREATE TABLE "mod_app.example.map_people"(id INTEGER PRIMARY KEY);
            CREATE TABLE "mod_app.example.other_places"(id INTEGER PRIMARY KEY);
            CREATE TABLE "mod_app.example.map_bad-name"(id INTEGER PRIMARY KEY);
            CREATE TABLE plain_table(id INTEGER);
            "#,
        )
        .unwrap();
    }

    let data = ModuleData::open(&path).unwrap();
    assert_eq!(
        data.tables_of("app.example.map").unwrap(),
        vec!["mod_app.example.map_people".to_string(), "mod_app.example.map_places".to_string()]
    );
    assert_eq!(data.table_count("app.example.map").unwrap(), 2);
    assert_eq!(
        data.tables_of("app.example.other").unwrap(),
        vec!["mod_app.example.other_places".to_string()]
    );
    assert!(data.tables_of("app.example.none").unwrap().is_empty());
    assert_eq!(data.table_count("app.example.none").unwrap(), 0);
}

#[test]
fn table_names_are_classified_by_prefix_and_suffix() {
    assert!(ModuleData::is_module_table("app.example.map", "mod_app.example.map_places"));
    assert!(ModuleData::is_module_table("app.example.map", "mod_app.example.map_p2"));
    assert!(!ModuleData::is_module_table("app.example.map", "mod_app.example.map_"));
    assert!(!ModuleData::is_module_table("app.example.map", "mod_app.example.map_bad-name"));
    assert!(!ModuleData::is_module_table("app.example.map", "mod_app.example.other_places"));
    assert!(!ModuleData::is_module_table("app.example.map", "module_schema"));
    assert!(!ModuleData::is_module_table("app.example.map", "mod_app.example.map_Places"));
}

#[test]
fn a_snapshot_is_a_consistent_copy() {
    let (dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.record_schema("app.example.map", 1).unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE \"mod_app.example.map_places\"(id INTEGER PRIMARY KEY)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO \"mod_app.example.map_places\"(id) VALUES (1)", [])
            .unwrap();
    }

    let snapshot = dir.path().join("snapshot.db");
    data.snapshot_into(&snapshot).unwrap();
    let copy = rusqlite::Connection::open(&snapshot).unwrap();
    let count: i64 = copy
        .query_row("SELECT COUNT(*) FROM \"mod_app.example.map_places\"", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let check: String = copy.query_row("PRAGMA quick_check", [], |row| row.get(0)).unwrap();
    assert_eq!(check, "ok");

    assert_eq!(
        data.snapshot_into(&snapshot).unwrap_err().code(),
        codes::MODULE_SNAPSHOT_FAILED,
        "目标已存在时不许覆盖"
    );
}

#[test]
fn the_module_database_sits_beside_the_core_database() {
    let core = PathBuf::from("data").join("yanmo.db");
    assert_eq!(ModuleData::path_beside(&core), PathBuf::from("data").join("modules.db"));
}
