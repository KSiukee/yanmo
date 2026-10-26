//! 模块数据体检与隔离验收：**看得见坏、关得住坏、救得回来**。
//!
//! 这一组只测核心承诺的五件事：
//! 1. 谁也认不出的表数得出来（不自动删）；
//! 2. 记了版本却没有表 = 数据丢了（damaged）；
//! 3. KV 表结构不对 = damaged；
//! 4. 隔离之后读、写、迁移全停，但**导出与体检照旧**；
//! 5. 隔离跨重开还在，解除与清除数据都能把它收掉。

use std::path::PathBuf;

use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::model::{ModuleColumn, ModuleColumnType, ModuleMigration, ModuleSchemaStep, ModuleTable};
use yanmo_core::store::ModuleData;

fn fresh() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.db");
    (dir, path)
}

fn issue_codes(data: &ModuleData, module_id: &str) -> Vec<&'static str> {
    data.audit(module_id)
        .unwrap()
        .issues
        .into_iter()
        .map(|issue| issue.code)
        .collect()
}

#[test]
fn a_healthy_module_audits_clean() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put("app.example.map", "theme", b"dark").unwrap();

    let audit = data.audit("app.example.map").unwrap();
    assert_eq!(audit.verdict, "ok");
    assert_eq!(audit.integrity, "clean");
    assert_eq!(audit.integrity_raw, "ok");
    assert_eq!(audit.tables, 1);
    assert_eq!(audit.rows, 1);
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(!audit.isolated);
    assert!(!audit.degraded);
    assert_eq!(audit.stray_tables, 0);
}

#[test]
fn tables_nobody_owns_are_counted_and_blamed_on_nobody() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put("app.example.map", "k", b"v").unwrap();
    {
        // 三种野表：后缀带连字符（像本模块的）、id 只有一段、跟模块无关的普通表
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE "mod_app.example.map_bad-name"(id INTEGER);
            CREATE TABLE "mod_bogus_table"(id INTEGER);
            CREATE TABLE "plain_table"(id INTEGER);
            "#,
        )
        .unwrap();
    }

    let audit = data.audit("app.example.map").unwrap();
    assert_eq!(audit.verdict, "damaged", "本模块前缀下有认不出的表");
    assert_eq!(issue_codes(&data, "app.example.map"), vec!["junk_table"]);
    assert_eq!(audit.stray_tables, 3, "三张表都该被数进「谁也不认」");
    // 别人的野表不栽到无关模块头上
    assert!(data.audit("app.example.other").unwrap().issues.is_empty());
}

#[test]
fn a_schema_version_without_any_table_is_damage() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.record_schema("app.example.lost", 3).unwrap();

    let audit = data.audit("app.example.lost").unwrap();
    assert_eq!(audit.verdict, "damaged");
    assert_eq!(issue_codes(&data, "app.example.lost"), vec!["lost_tables"]);
    assert_eq!(audit.tables, 0);
}

#[test]
fn a_kv_table_with_the_wrong_shape_is_damage() {
    let (_dir, path) = fresh();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(r#"CREATE TABLE "mod_app.example.broken_kv"(id INTEGER);"#).unwrap();
    }
    let data = ModuleData::open(&path).unwrap();

    let audit = data.audit("app.example.broken").unwrap();
    assert_eq!(audit.verdict, "damaged");
    assert_eq!(issue_codes(&data, "app.example.broken"), vec!["kv_shape"]);
    assert!(audit.issues[0].detail.contains("key"), "{}", audit.issues[0].detail);
}

#[test]
fn isolation_stops_reads_writes_and_migrations_but_not_export() {
    let (dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put("app.example.map", "k", b"v").unwrap();

    data.isolate("app.example.map", "audit found junk").unwrap();
    for error in [
        data.kv_get("app.example.map", "k").unwrap_err(),
        data.kv_put("app.example.map", "k", b"v2").unwrap_err(),
        data.kv_delete("app.example.map", "k").unwrap_err(),
    ] {
        assert_eq!(error.code(), codes::MODULE_DATA_ISOLATED);
    }

    let migration = ModuleMigration {
        version: 1,
        steps: vec![ModuleSchemaStep::CreateTable {
            table: ModuleTable {
                name: "extra".to_string(),
                columns: vec![ModuleColumn {
                    name: "id".to_string(),
                    kind: ModuleColumnType::Integer,
                    not_null: true,
                    primary_key: true,
                }],
            },
        }],
    };
    assert_eq!(
        data.apply_migration("app.example.map", &migration).unwrap_err().code(),
        codes::MODULE_DATA_ISOLATED
    );

    // 隔离不是删除：体检与导出照旧，数据一个字节都没少
    let audit = data.audit("app.example.map").unwrap();
    assert!(audit.isolated);
    assert_eq!(audit.isolation_reason, "audit found junk");
    let target = dir.path().join("salvage.db");
    data.export_namespace("app.example.map", &target).unwrap();
    let salvaged = ModuleData::open(&target).unwrap();
    // 导出的是同一份字节；新库没带隔离状态，所以能直接读
    assert_eq!(salvaged.kv_get("app.example.map", "k").unwrap(), Some(b"v".to_vec()));

    // 别的模块完全不受影响
    data.kv_put("app.example.other", "k", b"v").unwrap();
}

#[test]
fn isolation_survives_reopen_and_is_cleared_with_the_data() {
    let (_dir, path) = fresh();
    let mut first = ModuleData::open(&path).unwrap();
    first.kv_put("app.example.map", "k", b"v").unwrap();
    first.isolate("app.example.map", "manual").unwrap();

    let mut second = ModuleData::open(&path).unwrap();
    assert_eq!(
        second.isolation("app.example.map").unwrap().map(|state| state.reason),
        Some("manual".to_string())
    );
    assert_eq!(
        second.kv_put("app.example.map", "k", b"v2").unwrap_err().code(),
        codes::MODULE_DATA_ISOLATED
    );

    // 清除数据把隔离一起收掉：数据都没了，隔离没有对象
    assert_eq!(second.drop_namespace("app.example.map").unwrap(), 1);
    assert!(second.isolation("app.example.map").unwrap().is_none());
    second.kv_put("app.example.map", "k", b"fresh").unwrap();
}

#[test]
fn release_reports_whether_it_was_isolated() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    assert!(!data.release_isolation("app.example.map").unwrap(), "本来就没隔离");
    data.isolate("app.example.map", "").unwrap();
    assert!(data.release_isolation("app.example.map").unwrap());
    assert!(data.isolation("app.example.map").unwrap().is_none());
    data.kv_put("app.example.map", "k", b"v").unwrap();
}

#[test]
fn a_long_isolation_reason_is_trimmed_instead_of_failing() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    let reason = format!("  {}  ", "长".repeat(500));
    let state = data.isolate("app.example.map", &reason).unwrap();
    assert_eq!(state.reason.chars().count(), 200);
    assert!(!state.reason.starts_with(' '), "首尾空白要清掉");
}

/// 收工补镜像：变了才写（第二次是空的）；写失败不挡别人。
#[test]
fn only_changed_modules_get_a_fresh_mirror() {
    use yanmo_core::store::ModuleData;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.db");
    let mut data = ModuleData::open(&path).unwrap();
    // 写一次自己的数据 → 有了账，也变了
    data.kv_put("app.example.mirror", "notes.json", b"{\"a\":1}").unwrap();
    let written = data.refresh_mirrors_changed().unwrap();
    assert_eq!(written, vec!["app.example.mirror".to_string()]);
    assert_eq!(
        data.refresh_mirrors_changed().unwrap(),
        Vec::<String>::new(),
        "没变就不写"
    );
}
