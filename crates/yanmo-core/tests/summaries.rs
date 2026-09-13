//! 作品简介 / 每章一句话的验收：**两个手填字段，存的是作者原话**。
//!
//! 三条判据：
//! 1. 新库这两处是空串（空串＝没写过），写得进去读得回来；
//! 2. 老库打开**自动加列**，已有的正文与结构一个字不丢；
//! 3. 目录树一次查询就带上每章一句话（导出/大纲不该为每章各跑一趟）。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn summaries_start_empty_and_round_trip() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    assert_eq!(work.summary, "", "新书没写过简介：空串");

    // 写简介：作者写了什么就是什么（首尾空白也不动）
    store.set_work_summary(work.id, " 一个关于等待的故事。 ").unwrap();
    assert_eq!(store.get_work(work.id).unwrap().summary, " 一个关于等待的故事。 ");
    assert_eq!(store.list_works().unwrap()[0].summary, " 一个关于等待的故事。 ");

    // 每章一句话
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "第一章")
        .unwrap();
    let nodes = store.list_nodes(work.id).unwrap();
    let chapter_node = nodes.iter().find(|node| node.id == chapter).unwrap();
    assert_eq!(chapter_node.summary, "", "新章没写过：空串");

    store.set_node_summary(chapter, "他推开门，屋里没有人。").unwrap();
    let nodes = store.list_nodes(work.id).unwrap();
    let chapter_node = nodes.iter().find(|node| node.id == chapter).unwrap();
    assert_eq!(chapter_node.summary, "他推开门，屋里没有人。");
    // 单条读（打开章节时走这条）：与列表里那条一致
    assert_eq!(store.node_summary(chapter).unwrap(), "他推开门，屋里没有人。");

    // 改一次再读：覆盖写
    store.set_node_summary(chapter, "改了：屋里有个人。").unwrap();
    let nodes = store.list_nodes(work.id).unwrap();
    assert_eq!(
        nodes.iter().find(|node| node.id == chapter).unwrap().summary,
        "改了：屋里有个人。"
    );

    // 不存在的节点：明确报错，不静默成功
    assert!(store.set_node_summary(9999, "没有这一章").is_err());
    assert!(store.set_work_summary(9999, "没有这本书").is_err());
    assert!(store.node_summary(9999).is_err());
}

#[test]
fn an_old_database_gains_the_two_columns_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");

    // 造一个"上一版引擎留下的库"：只跑 v1，塞进书 + 章 + 正文
    {
        let conn = yanmo_core::db::open(&path).unwrap();
        for step in yanmo_core::db::migrations::MIGRATIONS[0].steps {
            conn.execute(step, []).unwrap();
        }
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO works(id, kind, title, created_at, updated_at, opened_at)
             VALUES(1, 'novel', '旧稿', 1, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id, work_id, parent_id, node_kind, title, sort_order, word_count,
                 created_at, updated_at)
             VALUES(1, 1, NULL, 'chapter', '旧章', 0, 0, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO node_contents(node_id, body, content_hash, char_count, updated_at)
             VALUES(1, '旧库里已经有正文了。', '', 0, 1)",
            [],
        )
        .unwrap();
    }

    // 新版打开：结构升到最新，两个新列就位，存量内容一个字没丢
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version(),
        "打开旧库应当升到最新结构版本"
    );
    assert_eq!(store.get_work(1).unwrap().summary, "", "升上来的书：简介是空串");
    assert_eq!(store.list_nodes(1).unwrap()[0].summary, "", "升上来的章：一句话是空串");
    assert_eq!(store.read_body(1).unwrap(), "旧库里已经有正文了。");

    // 升上来的库照样能写
    store.set_work_summary(1, "老树发新芽。").unwrap();
    store.set_node_summary(1, "旧章的一句话。").unwrap();
    assert_eq!(store.get_work(1).unwrap().summary, "老树发新芽。");
    assert_eq!(store.list_nodes(1).unwrap()[0].summary, "旧章的一句话。");
}
