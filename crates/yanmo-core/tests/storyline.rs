//! 故事总纲验收：**一本书一段话**（v14 的新列）的读、写、与"老库怎么长出来"。
//!
//! 三件事：
//! 1. 存的是**作者的原话**（不 trim、不动换行；空串＝没写过）；
//! 2. v14 之前的库（没有 `storyline` 列）打开后**长出这一列**，作者的稿子一字不动；
//! 3. 迁移重跑（升级到一半断电那种）幂等——总纲与稿子都还在。

use yanmo_core::db;
use yanmo_core::error::codes;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn the_storyline_is_stored_verbatim_and_defaults_to_empty() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    assert_eq!(store.work_storyline(work.id).unwrap(), "", "新建的书：一段都没写过");

    let text = "题材：悬疑。\n\n主线：他要把那盏灯等的那个人找回来。\n卖点：\n  ——没人知道灯下坐的是谁。";
    let saved = store.set_work_storyline(work.id, text).unwrap();
    assert_eq!(saved, text, "存的是作者的原话：换行与行首空格都不许被修剪");
    assert_eq!(store.work_storyline(work.id).unwrap(), text);

    // 作品简介与总纲是两格，互不影响
    store.set_work_summary(work.id, "一个关于等待的故事。").unwrap();
    assert_eq!(store.get_work(work.id).unwrap().storyline, text);
    assert_eq!(store.get_work(work.id).unwrap().summary, "一个关于等待的故事。");

    // 写不存在 / 已软删的书：如实拒
    let err = store.set_work_storyline(9999, "谁").unwrap_err();
    assert_eq!(err.code(), codes::WORK_GONE, "{err:?}");
    store.soft_delete_work(work.id).unwrap();
    assert_eq!(store.work_storyline(work.id).unwrap_err().code(), codes::WORK_GONE);
}

/// 一部**真的按 v13 形状**摆回去的库（把 v14 加的那一列删掉 + 版本号退一格）。
fn downgrade_to_v13(path: &std::path::Path) {
    let conn = db::open(path).unwrap();
    conn.execute("ALTER TABLE works DROP COLUMN storyline", []).unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();
}

#[test]
fn a_library_from_before_v14_gains_the_column_without_losing_a_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");

    let (work_id, chapter) = {
        let mut store = Store::open(&path).unwrap();
        let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
        let volume = store.list_nodes(work.id).unwrap()[0].id;
        let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章 门").unwrap();
        store.write_body(chapter, "他推开门，风雪灌了进来。").unwrap();
        store.set_node_summary(chapter, "他第一次进城。").unwrap();
        store.set_work_summary(work.id, "一个关于等待的故事。").unwrap();
        (work.id, chapter)
    };

    downgrade_to_v13(&path);
    // 退回去之后，那一列真的不在（不是"我以为它不在"）
    {
        let conn = db::open(&path).unwrap();
        let has: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('works') WHERE name = 'storyline'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has, 0, "夹具该是一份没有 storyline 列的 v13 库");
    }

    // 用当前引擎打开：结构升级到最新，旧数据一字不动
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        db::migrations::user_version(store.conn()).unwrap(),
        db::migrations::schema_version(),
    );
    assert_eq!(store.work_storyline(work_id).unwrap(), "", "升级后是新列，默认空串");
    assert_eq!(store.read_body(chapter).unwrap(), "他推开门，风雪灌了进来。");
    assert_eq!(
        store.get_work(work_id).unwrap().summary,
        "一个关于等待的故事。",
        "简介那一格不许被升级动到"
    );
    assert_eq!(
        store.outline_rows(work_id).unwrap().iter().find(|row| row.node_id == chapter).unwrap().summary,
        "他第一次进城。"
    );

    // 升级之后能写能读
    store.set_work_storyline(work_id, "主线：等他回来。").unwrap();
    assert_eq!(store.work_storyline(work_id).unwrap(), "主线：等他回来。");
}

#[test]
fn rerunning_v14_keeps_the_storyline_and_the_manuscript() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let (work_id, chapter) = {
        let mut store = Store::open(&path).unwrap();
        let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
        let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
        store.write_body(chapter, "正文一字不动。").unwrap();
        store.set_work_storyline(work.id, "主线：一条。").unwrap();
        (work.id, chapter)
    };

    // 版本号退回上一格：v14 会再走一遍（`ALTER` 前先查列，本来就该幂等）
    {
        let conn = db::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 13).unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.work_storyline(work_id).unwrap(), "主线：一条。", "重跑不该把总纲抹掉");
    assert_eq!(store.read_body(chapter).unwrap(), "正文一字不动。");
}
