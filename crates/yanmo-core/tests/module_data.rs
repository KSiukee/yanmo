//! 模块数据区验收：**schema 版本账 + 命名空间识别**。
//!
//! 不测模块表的语义（那是模块自己的迁移），只测核心承诺的两件事：
//! 每个模块的 schema 版本记得住、每张表认得出是谁的。

use std::path::PathBuf;

use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::model::{ModuleColumn, ModuleColumnType, ModuleMigration, ModuleSchemaStep, ModuleTable};
use yanmo_core::store::{ModuleData, ModuleDataLimits};

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

#[test]
fn kv_round_trip_and_delete() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put("app.example.data", "theme", b"dark").unwrap();
    assert_eq!(
        data.kv_get("app.example.data", "theme").unwrap(),
        Some(b"dark".to_vec())
    );
    assert!(data.kv_delete("app.example.data", "theme").unwrap());
    assert_eq!(data.kv_get("app.example.data", "theme").unwrap(), None);
    assert!(!data.kv_delete("app.example.data", "theme").unwrap());
}

#[test]
fn row_write_and_rate_limits_are_enforced() {
    let (_dir, path) = fresh();
    let limits = ModuleDataLimits {
        max_rows_per_table: 1,
        max_write_bytes: 4,
        max_writes_per_minute: 1,
        ..ModuleDataLimits::default()
    };
    let mut data = ModuleData::open_with_limits(&path, limits).unwrap();
    data.kv_put("app.example.data", "a", b"abcd").unwrap();
    let too_large = data.kv_put("app.example.data", "b", b"abcde");
    assert_eq!(too_large.unwrap_err().code(), codes::MODULE_DATA_WRITE_LIMIT);
    let too_many_rows = data.kv_put("app.example.data", "b", b"x");
    assert_eq!(too_many_rows.unwrap_err().code(), codes::MODULE_DATA_ROW_LIMIT);

    let (_dir2, path2) = fresh();
    let limits = ModuleDataLimits {
        max_writes_per_minute: 1,
        ..ModuleDataLimits::default()
    };
    let mut data = ModuleData::open_with_limits(&path2, limits).unwrap();
    data.kv_put("app.example.data", "a", b"x").unwrap();
    let rate = data.kv_put("app.example.data", "b", b"x");
    assert_eq!(rate.unwrap_err().code(), codes::MODULE_DATA_RATE_LIMIT);
}

#[test]
fn table_limit_is_enforced_on_migrations() {
    let (_dir, path) = fresh();
    let limits = ModuleDataLimits { max_tables: 1, ..ModuleDataLimits::default() };
    let mut data = ModuleData::open_with_limits(&path, limits).unwrap();
    data.kv_put("app.example.data", "a", b"x").unwrap();
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
    let error = data.apply_migration("app.example.data", &migration).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_DATA_TABLE_LIMIT);
}

#[test]
fn database_hard_cap_rejects_growth() {
    let (_dir, path) = fresh();
    let limits = ModuleDataLimits {
        total_bytes: 16 * 1024,
        max_write_bytes: 64 * 1024,
        ..ModuleDataLimits::default()
    };
    let mut data = ModuleData::open_with_limits(&path, limits).unwrap();
    let error = data
        .kv_put("app.example.data", "big", &vec![0u8; 64 * 1024])
        .unwrap_err();
    assert!(
        format!("{error}").to_lowercase().contains("full")
            || error.code() == codes::MODULE_DATA_WRITE_LIMIT,
        "{error}"
    );
}

#[test]
fn per_module_kv_quota_is_enforced() {
    let (_dir, path) = fresh();
    let limits = ModuleDataLimits {
        max_module_bytes: 4,
        ..ModuleDataLimits::default()
    };
    let mut data = ModuleData::open_with_limits(&path, limits).unwrap();
    data.kv_put("app.example.data", "a", b"abcd").unwrap();
    let error = data.kv_put("app.example.data", "b", b"x").unwrap_err();
    assert_eq!(error.code(), codes::MODULE_DATA_MODULE_QUOTA);
}

#[test]
fn namespace_export_only_carries_one_module() {
    let (dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.kv_put("app.example.a", "k", b"a").unwrap();
    data.kv_put("app.example.b", "k", b"b").unwrap();
    let target = dir.path().join("only-a.db");
    data.export_namespace("app.example.a", &target).unwrap();
    let exported = ModuleData::open(&target).unwrap();
    assert_eq!(exported.kv_get("app.example.a", "k").unwrap(), Some(b"a".to_vec()));
    assert_eq!(exported.kv_get("app.example.b", "k").unwrap(), None);
}

/// 模块界面读自己的数据：`kv_keys` / `kv_get` 只看到自己的键。
#[test]
fn a_module_can_list_its_own_keys() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    assert_eq!(data.kv_keys("app.example.data").unwrap(), Vec::<String>::new(), "没写过就是空");
    data.kv_put("app.example.data", "map.json", b"{}").unwrap();
    data.kv_put("app.example.data", "theme", b"dark").unwrap();
    data.kv_put("app.example.other", "map.json", b"{}").unwrap();
    let keys = data.kv_keys("app.example.data").unwrap();
    assert_eq!(keys, vec!["map.json".to_string(), "theme".to_string()]);
    assert_eq!(keys.len(), 2, "别的模块的键不许混进来");
}

/// 唤起留痕：记一次完整的运行，能按模块读回来；别的模块看不到。
#[test]
fn a_run_event_remembers_who_woke_it_and_how_long_it_took() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.note_run_result("app.example.data", "user", 1_700_000_000_000, 412, "ok", "")
        .unwrap();
    data.note_run_result("app.example.data", "chain:app.example.other", 1_700_000_001_000, 90, "error", "host.trap")
        .unwrap();
    data.note_run_result("app.example.other", "user", 1_700_000_002_000, 10, "ok", "")
        .unwrap();
    let runs = data.recent_runs("app.example.data", 10).unwrap();
    assert_eq!(runs.len(), 2, "别的模块的记录不许混进来");
    assert_eq!(runs[0].trigger, "chain:app.example.other", "新的在前");
    assert_eq!(runs[0].outcome, "error");
    assert_eq!(runs[0].code, "host.trap");
    assert_eq!(runs[1].duration_ms, 412);
    assert!(data.recent_runs("app.example.none", 10).unwrap().is_empty());
}

/// 断链：断了就在、重启还在、接回去才通；自环没有意义。
#[test]
fn a_cut_edge_stays_cut_until_the_author_restores_it() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    assert!(!data.is_cut("app.a", "app.b").unwrap());
    data.cut_edge("app.a", "app.b").unwrap();
    data.cut_edge("app.a", "app.b").unwrap(); // 幂等
    assert!(data.is_cut("app.a", "app.b").unwrap());
    assert!(!data.is_cut("app.b", "app.a").unwrap(), "反向是另一条边");
    assert_eq!(data.cuts().unwrap().len(), 1);

    // 重新打开（= 重启）仍然断着
    drop(data);
    let reopened = ModuleData::open(&path).unwrap();
    assert!(reopened.is_cut("app.a", "app.b").unwrap());

    let mut reopened = reopened;
    assert!(reopened.restore_edge("app.a", "app.b").unwrap(), "接回要说明真接回了");
    assert!(!reopened.is_cut("app.a", "app.b").unwrap());
    assert!(!reopened.restore_edge("app.a", "app.b").unwrap(), "再接一次就没得接了");

    assert!(reopened.cut_edge("app.a", "app.a").is_err(), "自环拒掉");
}

/// 边只看"真走过"的：留痕里有 chain: 行才画得出来；断了的状态合并进来。
#[test]
fn edges_come_from_real_runs_and_carry_the_cut_flag() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.note_run_result("app.b", "chain:app.a", 1_000, 10, "ok", "").unwrap();
    data.note_run_result("app.b", "chain:app.a", 2_000, 10, "error", "host.trap").unwrap();
    data.note_run_result("app.a", "user", 3_000, 10, "ok", "").unwrap();
    let edges = data.edges().unwrap();
    assert_eq!(edges.len(), 1, "只有 chain 来源的才算边");
    assert_eq!(edges[0].from, "app.a");
    assert_eq!(edges[0].to, "app.b");
    assert_eq!(edges[0].runs, 2);
    assert_eq!(edges[0].failures, 1);
    assert!(!edges[0].cut);

    data.cut_edge("app.a", "app.b").unwrap();
    assert!(data.edges().unwrap()[0].cut, "断了要在边上看得出来");
}

/// 退避：失败就往后推（指数、封顶），成功立刻清零；重启不清。
#[test]
fn backoff_grows_on_failures_and_clears_on_success() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    assert!(data.backoff_allows("app.b").unwrap(), "没失败过就放行");

    data.note_backoff("app.b", false).unwrap();
    let first = data.backoff("app.b").unwrap().unwrap();
    assert_eq!(first.failures, 1);
    assert!(!data.backoff_allows("app.b").unwrap(), "刚失败要等");
    let wait1 = first.next_at - yanmo_core::time::now_millis();
    assert!(wait1 > 5_000, "第一次大约等 10 秒（实际 {wait1}ms）");

    data.note_backoff("app.b", false).unwrap();
    let second = data.backoff("app.b").unwrap().unwrap();
    assert_eq!(second.failures, 2);
    assert!(second.next_at > first.next_at, "第二次等更久");

    // 重启不清：重开之后仍记得
    drop(data);
    let mut reopened = ModuleData::open(&path).unwrap();
    assert_eq!(reopened.backoff("app.b").unwrap().unwrap().failures, 2);

    reopened.note_backoff("app.b", true).unwrap();
    assert!(reopened.backoff("app.b").unwrap().is_none(), "成功一次就清零");
    assert!(reopened.backoff_allows("app.b").unwrap());
}

/// 每模块退避参数：默认 10 秒起 / 5 分钟封顶；改了按改的算；关掉就不挡；校验挡坏值。
#[test]
fn retry_policy_is_per_module_and_validated() {
    use yanmo_core::store::RetryPolicy;
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    let default = data.retry_policy("app.b").unwrap();
    assert_eq!(default.base_ms, 10_000);
    assert_eq!(default.max_ms, 300_000);
    assert!(default.enabled);

    data.set_retry_policy(
        "app.b",
        &RetryPolicy { base_ms: 2_000, max_ms: 30_000, enabled: true },
    )
    .unwrap();
    assert_eq!(data.retry_policy("app.b").unwrap().base_ms, 2_000);
    assert_eq!(data.retry_policy("app.c").unwrap().base_ms, 10_000, "别的模块不受影响");

    data.note_backoff("app.b", false).unwrap();
    let wait = data.backoff("app.b").unwrap().unwrap().next_at - yanmo_core::time::now_millis();
    assert!(wait <= 2_000 + 500, "按模块的起步算（实际 {wait}ms）");

    data.set_retry_policy(
        "app.b",
        &RetryPolicy { base_ms: 1_000, max_ms: 60_000, enabled: false },
    )
    .unwrap();
    assert!(data.backoff_allows("app.b").unwrap(), "关掉就不挡自动唤起");

    assert!(data
        .set_retry_policy(
            "app.b",
            &RetryPolicy { base_ms: 500, max_ms: 1_000, enabled: true },
        )
        .is_err(), "起步 500ms 太短，要拒");

    data.clear_retry_policy("app.b").unwrap();
    assert_eq!(data.retry_policy("app.b").unwrap().base_ms, 10_000, "清掉回默认");
}
