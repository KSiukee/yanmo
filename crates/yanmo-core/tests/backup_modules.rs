//! 备份带上模块数据：**两个库一起进包、一起验、一起恢复；不带时不许动现有模块数据**。
//!
//! 这一组只走"包目录 + 恢复"那条路（加密包由整目录封包自动包含，验包与恢复共用同一套判定）。

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use yanmo_core::model::{
    ModuleColumn, ModuleColumnType, ModuleMigration, ModuleSchemaStep, ModuleTable, WorkKind,
};
use yanmo_core::store::{
    read_manifest, swap_in, BackupConfig, BackupRequest, BackupTarget, ModuleData, Store,
};

const MODULE_ID: &str = "app.example.map";

struct Fixture {
    _dir: TempDir,
    data_dir: PathBuf,
    store: Store,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let mut store = Store::open(data_dir.join("yanmo.db")).unwrap();
        store.create_work(WorkKind::Novel, "题本").unwrap();
        {
            let mut modules = ModuleData::open(data_dir.join("modules.db")).unwrap();
            modules
                .apply_migration(
                    MODULE_ID,
                    &ModuleMigration {
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
                                        name: "label".to_string(),
                                        kind: ModuleColumnType::Text,
                                        not_null: false,
                                        primary_key: false,
                                    },
                                ],
                            },
                        }],
                    },
                )
                .unwrap();
        }
        // 用另一条连接写一行（ModuleData 在作用域结束时已经收掉，WAL 也 checkpoint 了）
        let conn = rusqlite::Connection::open(data_dir.join("modules.db")).unwrap();
        conn.execute(
            "INSERT INTO \"mod_app.example.map_places\"(label) VALUES ('甲')",
            [],
        )
        .unwrap();
        drop(conn);
        Self { _dir: dir, data_dir, store }
    }

    fn backup_to(&mut self, target: &Path, include_modules: bool) -> PathBuf {
        std::fs::create_dir_all(target).unwrap();
        let request = BackupRequest {
            seal: None,
            data_dir: self.data_dir.clone(),
            targets: vec![BackupTarget {
                path: target.to_string_lossy().to_string(),
                volume_id: String::new(),
                volume_label: String::new(),
                removable: false,
            }],
            keep: 7,
            tz_offset_minutes: 480,
            device: "测试机".to_string(),
            include_modules,
            manual: false,
        };
        let report = self.store.backup_now(&request).unwrap();
        assert_eq!(report.succeeded(), 1, "备份应当成功：{report:?}");
        let package = PathBuf::from(&report.outcomes[0].package);
        assert!(self.store.verify_backup(&package).ok, "备份应当通过体检");
        package
    }
}

fn place_count(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM \"mod_app.example.map_places\"",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn a_backup_with_modules_carries_both_databases_and_restores_them_together() {
    let mut fixture = Fixture::new();
    let target = fixture._dir.path().join("backup");
    let package = fixture.backup_to(&target, true);

    let manifest = read_manifest(&package).unwrap();
    let stamp = manifest.modules.expect("清单应当带模块数据戳");
    assert_eq!(stamp.schemas.get(MODULE_ID), Some(&1));
    assert!(stamp.bytes > 0);
    assert!(package.join("modules.db").is_file());

    let restored_dir = fixture._dir.path().join("restored");
    let outcome = swap_in(&package, &restored_dir, "20261024-1500").unwrap();
    assert!(outcome.modules_restored, "这份备份带了模块数据，恢复时应当一起换");
    assert_eq!(place_count(&restored_dir.join("modules.db")), 1);
    let restored = Store::open(restored_dir.join("yanmo.db")).unwrap();
    assert_eq!(restored.list_works().unwrap().len(), 1);
}

#[test]
fn a_backup_without_modules_leaves_the_live_module_data_alone() {
    let mut fixture = Fixture::new();
    let target = fixture._dir.path().join("backup-no-modules");
    let package = fixture.backup_to(&target, false);
    let manifest = read_manifest(&package).unwrap();
    assert!(manifest.modules.is_none());
    assert!(!package.join("modules.db").exists());

    // 恢复到一个已经有一份模块数据的目录：那份必须原样留着
    let restored_dir = fixture._dir.path().join("restored-no-modules");
    std::fs::create_dir_all(&restored_dir).unwrap();
    std::fs::copy(fixture.data_dir.join("modules.db"), restored_dir.join("modules.db")).unwrap();
    let outcome = swap_in(&package, &restored_dir, "20261024-1501").unwrap();
    assert!(!outcome.modules_restored);
    assert_eq!(place_count(&restored_dir.join("modules.db")), 1, "现有模块数据不许被覆盖");
}

#[test]
fn a_corrupt_module_snapshot_fails_verification() {
    let mut fixture = Fixture::new();
    let target = fixture._dir.path().join("backup-corrupt");
    let package = fixture.backup_to(&target, true);
    std::fs::write(package.join("modules.db"), b"this is not a database").unwrap();
    let verify = fixture.store.verify_backup(&package);
    assert!(!verify.ok);
    assert!(
        verify.problems.iter().any(|problem| problem.contains("模块")),
        "问题要说清是模块数据：{:?}",
        verify.problems
    );
}

#[test]
fn backup_config_includes_modules_by_default() {
    assert!(BackupConfig::default().include_modules);
    let parsed: BackupConfig = serde_json::from_str(r#"{"targets":[],"keep":7,"auto_on_start":true,"auto_on_close":true}"#).unwrap();
    assert!(parsed.include_modules, "老配置没有这一格时默认带模块数据");
}
