//! 大纲表读数验收：**整棵树铺平、一行带上该有的列**。
//!
//! 四件事：
//! 1. 树序 + 缩进层级（卷 0 / 章 1 / 场景卡 2）；
//! 2. 四格跟着节点走（没填过的就是四个空串）；
//! 3. 伏笔账按"埋在哪一章 / 收在哪一章"数（只看埋着的算"待收"）；
//! 4. 渲染后的章名与字数、有没有正文都带着（界面不再为一屏发四次请求）。

use yanmo_core::model::{
    ForeshadowState, NewForeshadow, NodeKind, SceneFields, WorkKind,
};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn the_outline_table_carries_the_tree_order_depth_and_columns() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let scene = store.create_node(work.id, Some(second), NodeKind::Scene, "对峙").unwrap();

    // 第二张场景卡填一格四格；章写一句话
    store
        .save_scene_fields(
            &SceneFields {
                node_id: scene,
                pov: "陆文".to_string(),
                goal: String::new(),
                conflict: String::new(),
                outcome: String::new(),
            },
            "test",
        )
        .unwrap();
    store.set_node_summary(first, "他第一次进城").unwrap();

    // 伏笔：第 1 章埋两条（一条收了），第 2 章收一条
    let mut draft = NewForeshadow {
        work_id: work.id,
        body: "老张的怀表".to_string(),
        planted_node: Some(first),
        note: String::new(),
    };
    let open = store.create_foreshadow(&draft, "test").unwrap();
    let closed = store.create_foreshadow(&draft, "test").unwrap();
    store
        .move_foreshadow(closed, ForeshadowState::Collected, Some(second), "test")
        .unwrap();
    draft.planted_node = Some(second);
    store.create_foreshadow(&draft, "test").unwrap();

    let rows = store.outline_rows(work.id).unwrap();
    // 卷 → 第1章 → 第2章 → 场景卡（树序）
    assert_eq!(rows.len(), 4, "{rows:#?}");
    assert_eq!(rows[0].node_id, volume);
    assert_eq!(rows[0].depth, 0);
    assert_eq!(rows[1].node_id, first);
    assert_eq!(rows[1].depth, 1);
    assert_eq!(rows[2].node_id, second);
    assert_eq!(rows[3].node_id, scene);
    assert_eq!(rows[3].depth, 2, "场景卡比它所属的章深一层");
    assert_eq!(rows[3].parent_id, Some(second));

    // 列：渲染后的章名 / 一句话 / 字数 / 有没有正文 / 四格
    assert_eq!(rows[1].title, "第1章", "标题里存的是模板，给出来的是渲染后的号");
    assert_eq!(rows[1].summary, "他第一次进城");
    assert_eq!(rows[1].word_count, 0);
    assert!(!rows[1].has_body);
    assert!(rows[1].fields.missing().len() == 4, "章没填四格：四个空串");
    assert_eq!(rows[3].fields.pov, "陆文");
    assert_eq!(rows[3].title, "对峙", "场景卡不是章，名字原样给");

    // 伏笔账：第 1 章还埋着 1 条（另一条已收）、收掉 0 条；第 2 章埋着 1 条、收掉 1 条
    assert_eq!((rows[1].planted_open, rows[1].collected), (1, 0));
    assert_eq!((rows[2].planted_open, rows[2].collected), (1, 1));
    assert_eq!((rows[3].planted_open, rows[3].collected), (0, 0), "场景卡上没有伏笔");

    // 收了的那条不再算"待收"
    store
        .move_foreshadow(open, ForeshadowState::Collected, Some(first), "test")
        .unwrap();
    let rows = store.outline_rows(work.id).unwrap();
    assert_eq!((rows[1].planted_open, rows[1].collected), (0, 1));
}

/// 空书也要能读（表里就一行都没有，界面显示"先建一章"）。
#[test]
fn an_empty_book_reads_as_an_empty_table() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let rows = store.outline_rows(work.id).unwrap();
    // 新建长篇时核心会顺手给一卷
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, NodeKind::Volume);
}
