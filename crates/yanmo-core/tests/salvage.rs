//! 坏库打捞的验收：能搬的都搬过去、源库一个字节不动、目标已存在就拒绝。
//!
//! 真正的"页坏了"场景没法在单测里稳定造出来（要精确踩在 b-tree 页上），
//! 这里钉住的是打捞的**安全边界**与**保真度**；页级坏掉的逐行补捞在真机上验。

use std::path::Path;

use yanmo_core::model::{Author, WorkKind};
use yanmo_core::store::Appearance;
use yanmo_core::store::{salvage_database, Store};

fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("yanmo.db");
    (dir, source)
}

#[test]
fn salvage_copies_readable_rows_into_a_fresh_database() {
    let (dir, source) = fresh();
    {
        let mut store = Store::open(&source).unwrap();
        let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
        store
            .set_author(&Author {
                pen_name: Some("全局笔名".to_string()),
                real_name: Some("真名".to_string()),
                ..Default::default()
            })
            .unwrap();
        store
            .set_appearance(
                Some(work.id),
                &Appearance { editor_font_size: Some(20), ..Default::default() },
            )
            .unwrap();
    }
    let original = std::fs::read(&source).unwrap();

    let target = dir.path().join("recovered.db");
    let report = salvage_database(&source, &target).unwrap();

    assert!(target.is_file(), "新库要真的写出来");
    assert!(report.integrity.iter().any(|line| line == "ok"), "新库体检要过：{:?}", report.integrity);
    assert!(
        report.tables.iter().any(|table| table.name == "works" && table.status == "copied"),
        "作品表要整表搬过去：{:?}",
        report.tables
    );

    // 源库一个字节都不能变
    assert_eq!(original, std::fs::read(&source).unwrap(), "打捞不许改源库");

    // 新库打开后，作品、作者信息、每书外观都在
    let mut recovered = Store::open(&target).unwrap();
    assert_eq!(recovered.list_works().unwrap()[0].title, "长夜");
    assert_eq!(recovered.author().unwrap().pen_name, "全局笔名");
    let work_id = recovered.list_works().unwrap()[0].id;
    assert_eq!(
        recovered.appearance(Some(work_id)).unwrap().editor_font_size,
        Some(20)
    );
    let ops: i64 = recovered
        .conn()
        .query_row("SELECT COUNT(*) FROM op_log", [], |row| row.get(0))
        .unwrap();
    assert!(ops > 0, "操作留痕也要带过来");
}

#[test]
fn salvage_refuses_to_overwrite_an_existing_target() {
    let (dir, source) = fresh();
    {
        let mut store = Store::open(&source).unwrap();
        store.create_work(WorkKind::Novel, "长夜").unwrap();
    }
    let target = dir.path().join("already.db");
    std::fs::write(&target, b"occupied").unwrap();
    assert!(salvage_database(&source, &target).is_err(), "目标存在必须拒绝，不许覆盖");
    assert_eq!(std::fs::read(&target).unwrap(), b"occupied", "拒绝时目标原样保留");
}

#[test]
fn salvage_reports_a_missing_source_instead_of_creating_one() {
    let (dir, _source) = fresh();
    let target = dir.path().join("recovered.db");
    assert!(salvage_database(Path::new(&dir.path().join("nope.db")), &target).is_err());
    assert!(!target.exists(), "源不存在时不该留下半成品");
}
