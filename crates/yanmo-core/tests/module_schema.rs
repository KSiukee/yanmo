//! 模块 schema 迁移验收：**逐版、可回滚、没有任意 SQL**。
//!
//! 这一组只测核心的执行器（建表 / 加列 / 建索引与版本账），不测模块自己的业务表语义。

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::model::{
    ModuleColumn, ModuleColumnType, ModuleMigration, ModuleSchemaStep, ModuleTable,
};
use yanmo_core::store::{ModuleData, ModuleStats};

const ID: &str = "app.example.map";

fn fresh() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("modules.db");
    (dir, path)
}

fn column(name: &str, kind: ModuleColumnType) -> ModuleColumn {
    ModuleColumn {
        name: name.to_string(),
        kind,
        not_null: false,
        primary_key: false,
    }
}

fn places() -> ModuleTable {
    ModuleTable {
        name: "places".to_string(),
        columns: vec![
            ModuleColumn {
                name: "id".to_string(),
                kind: ModuleColumnType::Integer,
                not_null: true,
                primary_key: true,
            },
            column("label", ModuleColumnType::Text),
            column("lat", ModuleColumnType::Real),
        ],
    }
}

fn create_places() -> ModuleSchemaStep {
    ModuleSchemaStep::CreateTable { table: places() }
}

fn migration(version: i64, steps: Vec<ModuleSchemaStep>) -> ModuleMigration {
    ModuleMigration { version, steps }
}

#[test]
fn v1_creates_the_table_and_index_inside_the_namespace() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.apply_migration(
        ID,
        &migration(
            1,
            vec![
                create_places(),
                ModuleSchemaStep::CreateIndex {
                    name: "by_label".to_string(),
                    table: "places".to_string(),
                    columns: vec!["label".to_string()],
                    unique: true,
                },
            ],
        ),
    )
    .unwrap();
    assert_eq!(data.schema_version(ID).unwrap(), Some(1));
    assert_eq!(
        data.tables_of(ID).unwrap(),
        vec!["mod_app.example.map_places".to_string()]
    );

    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("PRAGMA table_info(\"mod_app.example.map_places\")")
        .unwrap();
    let columns: Vec<(String, String, i64, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get(1)?, row.get(2)?, row.get(3)?, row.get(5)?))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(
        columns,
        vec![
            ("id".to_string(), "INTEGER".to_string(), 1, 1),
            ("label".to_string(), "TEXT".to_string(), 0, 0),
            ("lat".to_string(), "REAL".to_string(), 0, 0),
        ]
    );

    let unique: i64 = conn
        .query_row(
            "SELECT \"unique\" FROM pragma_index_list('mod_app.example.map_places')
              WHERE name = 'mod_app.example.map_by_label'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unique, 1);
}

#[test]
fn v2_adds_a_column_without_losing_rows() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.apply_migration(ID, &migration(1, vec![create_places()])).unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO \"mod_app.example.map_places\"(label) VALUES ('甲')",
            [],
        )
        .unwrap();
    }
    data.apply_migration(
        ID,
        &migration(
            2,
            vec![ModuleSchemaStep::AddColumn {
                table: "places".to_string(),
                column: column("note", ModuleColumnType::Text),
            }],
        ),
    )
    .unwrap();
    assert_eq!(data.schema_version(ID).unwrap(), Some(2));

    let conn = rusqlite::Connection::open(&path).unwrap();
    let (label, note): (String, Option<String>) = conn
        .query_row(
            "SELECT label, note FROM \"mod_app.example.map_places\"",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(label, "甲");
    assert_eq!(note, None);
}

#[test]
fn versions_must_advance_one_at_a_time() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.apply_migration(ID, &migration(1, vec![create_places()])).unwrap();

    for version in [1, 3] {
        let error = data
            .apply_migration(
                ID,
                &migration(
                    version,
                    vec![ModuleSchemaStep::AddColumn {
                        table: "places".to_string(),
                        column: column("note", ModuleColumnType::Text),
                    }],
                ),
            )
            .unwrap_err();
        assert_eq!(error.code(), codes::MODULE_SCHEMA_VERSION);
        assert_eq!(data.schema_version(ID).unwrap(), Some(1), "版本不得前进");
    }

    data.apply_migration(
        ID,
        &migration(
            2,
            vec![ModuleSchemaStep::AddColumn {
                table: "places".to_string(),
                column: column("note", ModuleColumnType::Text),
            }],
        ),
    )
    .unwrap();
    assert_eq!(data.schema_version(ID).unwrap(), Some(2));
}

#[test]
fn bad_names_are_rejected_before_anything_touches_the_database() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();

    for bad in ["Places", "2places", "a-b", "a b", "x\"; DROP TABLE modules; --"] {
        let mut table = places();
        table.name = bad.to_string();
        let error = data
            .apply_migration(ID, &migration(1, vec![ModuleSchemaStep::CreateTable { table }]))
            .unwrap_err();
        assert_eq!(error.code(), codes::MODULE_SCHEMA_STEP_INVALID, "table: {bad:?}");
        assert_eq!(data.schema_version(ID).unwrap(), None);
        assert!(data.tables_of(ID).unwrap().is_empty());
    }

    let mut table = places();
    table.columns[1].name = "Label".to_string();
    let error = data
        .apply_migration(ID, &migration(1, vec![ModuleSchemaStep::CreateTable { table }]))
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_SCHEMA_STEP_INVALID);
}

#[test]
fn a_failed_step_rolls_back_the_whole_migration() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.apply_migration(ID, &migration(1, vec![create_places()])).unwrap();

    let mut second = places();
    second.name = "second".to_string();
    let error = data
        .apply_migration(
            ID,
            &migration(
                2,
                vec![
                    ModuleSchemaStep::CreateTable { table: second },
                    ModuleSchemaStep::AddColumn {
                        table: "missing".to_string(),
                        column: column("note", ModuleColumnType::Text),
                    },
                ],
            ),
        )
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_SCHEMA_FAILED);
    assert_eq!(data.schema_version(ID).unwrap(), Some(1), "失败不得推进版本");
    assert_eq!(
        data.tables_of(ID).unwrap(),
        vec!["mod_app.example.map_places".to_string()],
        "前一步建出来的表也要回滚掉"
    );
}

#[test]
fn two_modules_cannot_see_or_touch_each_others_tables() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.apply_migration(ID, &migration(1, vec![create_places()])).unwrap();
    data.apply_migration("app.example.other", &migration(1, vec![create_places()]))
        .unwrap();

    assert_eq!(data.tables_of(ID).unwrap(), vec!["mod_app.example.map_places".to_string()]);
    assert_eq!(
        data.tables_of("app.example.other").unwrap(),
        vec!["mod_app.example.other_places".to_string()]
    );
    assert_eq!(data.table_count(ID).unwrap(), 1);
    assert_eq!(data.table_count("app.example.other").unwrap(), 1);
}

#[test]
fn stats_count_tables_and_rows() {
    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    let mut notes = ModuleTable {
        name: "notes".to_string(),
        columns: vec![column("body", ModuleColumnType::Text)],
    };
    notes.columns.push(column("tag", ModuleColumnType::Text));
    data.apply_migration(
        ID,
        &migration(
            1,
            vec![create_places(), ModuleSchemaStep::CreateTable { table: notes }],
        ),
    )
    .unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        for label in ["甲", "乙"] {
            conn.execute(
                "INSERT INTO \"mod_app.example.map_places\"(label) VALUES (?1)",
                [label],
            )
            .unwrap();
        }
        for body in ["一", "二", "三"] {
            conn.execute("INSERT INTO \"mod_app.example.map_notes\"(body) VALUES (?1)", [body])
                .unwrap();
        }
    }

    let ModuleStats { tables, rows, db_bytes } = data.stats(ID).unwrap();
    assert_eq!(tables, 2);
    assert_eq!(rows, 5);
    assert!(db_bytes > 0);
    assert_eq!(data.stats("app.example.none").unwrap(), ModuleStats { tables: 0, rows: 0, db_bytes: db_bytes });
}

#[test]
fn migrations_can_be_parsed_from_the_host_json_shape() {
    let value = json!({
        "version": 1,
        "steps": [
            {
                "action": "create_table",
                "table": {
                    "name": "places",
                    "columns": [
                        { "name": "id", "kind": "integer", "primary_key": true },
                        { "name": "label", "kind": "text", "not_null": true }
                    ]
                }
            },
            {
                "action": "create_index",
                "name": "by_label",
                "table": "places",
                "columns": ["label"],
                "unique": false
            }
        ]
    });
    let migration: ModuleMigration = serde_json::from_value(value).unwrap();
    migration.validate().unwrap();

    let (_dir, path) = fresh();
    let mut data = ModuleData::open(&path).unwrap();
    data.apply_migration(ID, &migration).unwrap();
    assert_eq!(data.schema_version(ID).unwrap(), Some(1));
    assert_eq!(data.table_count(ID).unwrap(), 1);
}
