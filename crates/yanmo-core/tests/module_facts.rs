//! 模块事实（透明度面板的后端）验收：**数字对得上、口径说清楚、账不被数据操作带走**。
//!
//! 这一组盯四件事：字节怎么算（KV 口径） / 只有非 KV 表时如实标「不完整」 /
//! 隔离与降级照实报 / 使用账按模块分开且跨重开还在。

use std::path::PathBuf;

use tempfile::TempDir;
use yanmo_core::model::{ModuleColumn, ModuleColumnType, ModuleMigration, ModuleSchemaStep, ModuleTable};
use yanmo_core::store::ModuleData;

const MODULE: &str = "app.example.map";

fn fresh() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.db");
    (dir, path)
}

#[test]
fn facts_report_space_structure_and_usage() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "a", b"12").unwrap();
    data.kv_put(MODULE, "b", b"345").unwrap();
    data.record_schema(MODULE, 2).unwrap();
    data.note_run(MODULE).unwrap();
    data.note_run(MODULE).unwrap();

    let before = data.facts(MODULE).unwrap();
    assert_eq!(before.tables, 1);
    assert_eq!(before.rows, 2);
    assert_eq!(before.data_bytes, 7, "key 1+2 与 value 2+3 都要算进去");
    assert!(!before.data_bytes_partial);
    assert!(before.db_bytes > 0);
    assert_eq!(before.schema_version, Some(2));
    assert_eq!(before.runs, 2);
    assert!(before.last_run_at > 0);
    assert_eq!(before.mirror_written_at, 0, "还没写过镜像");

    data.mirror(MODULE).unwrap();
    assert!(data.facts(MODULE).unwrap().mirror_written_at > 0, "写过镜像就该有时刻");
}

#[test]
fn a_namespace_without_value_columns_says_the_byte_count_is_partial() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"vv").unwrap();
    let migration = ModuleMigration {
        version: 1,
        steps: vec![ModuleSchemaStep::CreateTable {
            table: ModuleTable {
                name: "places".to_string(),
                columns: vec![ModuleColumn {
                    name: "id".to_string(),
                    kind: ModuleColumnType::Integer,
                    not_null: true,
                    primary_key: true,
                }],
            },
        }],
    };
    data.apply_migration(MODULE, &migration).unwrap();

    let facts = data.facts(MODULE).unwrap();
    assert_eq!(facts.tables, 2);
    assert!(facts.data_bytes_partial, "有表没有 value 列：字节数只算了 KV");
    assert_eq!(facts.data_bytes, 3, "key 1 + value 2");
}

#[test]
fn facts_carry_health_and_isolation_as_they_are() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO module_data_health(module_id, degraded, rejected, updated_at)
             VALUES(?1, 1, 7, 0)",
            [MODULE],
        )
        .unwrap();
    }
    data.isolate(MODULE, "audit found junk").unwrap();

    let facts = data.facts(MODULE).unwrap();
    assert!(facts.degraded);
    assert_eq!(facts.rejected, 7);
    assert!(facts.isolated);
    assert_eq!(facts.isolation_reason, "audit found junk");
}

#[test]
fn usage_is_per_module_and_survives_reopen() {
    let (_dir, path) = fresh();
    {
        let mut data = ModuleData::open(&path).unwrap();
        data.note_run("app.example.a").unwrap();
        data.note_run("app.example.a").unwrap();
        data.note_run("app.example.b").unwrap();
    }
    let data = ModuleData::open(&path).unwrap();
    assert_eq!(data.usage("app.example.a").unwrap().runs, 2);
    assert_eq!(data.usage("app.example.b").unwrap().runs, 1);
    let untouched = data.usage("app.example.c").unwrap();
    assert_eq!((untouched.runs, untouched.last_run_at), (0, 0), "没跑过就是 0/0");
}

#[test]
fn clearing_module_data_keeps_the_usage_ledger() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put(MODULE, "k", b"v").unwrap();
    data.note_run(MODULE).unwrap();
    assert_eq!(data.drop_namespace(MODULE).unwrap(), 1);

    // 「跑过几次」说的是这个模块被启动过几次，不是它的数据；清数据不该把它抹掉
    assert_eq!(data.usage(MODULE).unwrap().runs, 1);
    assert_eq!(data.facts(MODULE).unwrap().tables, 0);
}
