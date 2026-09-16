//! 场景卡四格验收：**四格缺项是"值为空"，不是另一张布尔**。
//!
//! 这一份盯四件事：
//! 1. 不是场景卡就没有四格（如实拒，含"节点根本不在"与"在但不是场景"两种）；
//! 2. 没有那一行 = 四格全空（缺项检测要的就是这个）；
//! 3. 存了读回来一致（值修剪首尾空白），再存是整行覆盖；
//! 4. 按书列出场景卡时**带上四格**，顺序按树里的顺序（检测要按树读才顺）。

use yanmo_core::error::codes;
use yanmo_core::model::{NodeKind, SceneField, SceneFields, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 建一本书 + 一章 + 至少两张场景卡，返回 `(work_id, chapter_id, [场景卡 id])`。
fn seed() -> (tempfile::TempDir, Store, i64, Vec<i64>) {
    let (dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();
    let first = store
        .create_node(work.id, Some(chapter), NodeKind::Scene, "开场")
        .unwrap();
    let second = store
        .create_node(work.id, Some(chapter), NodeKind::Scene, "对峙")
        .unwrap();
    (dir, store, work.id, vec![first, second])
}

#[test]
fn a_node_that_is_not_a_scene_has_no_four_fields() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();

    let err = store.scene_fields(chapter).unwrap_err();
    assert_eq!(err.code(), codes::NODE_NOT_SCENE, "章没有那四格");

    let err = store.scene_fields(9999).unwrap_err();
    assert_eq!(err.code(), codes::NODE_GONE, "节点不在是另一回事（如实分开报）");
}

#[test]
fn a_scene_without_a_row_reads_as_four_blank_fields() {
    let (_dir, store, _work, scenes) = seed();
    let fields = store.scene_fields(scenes[0]).unwrap();
    assert_eq!(fields, SceneFields::empty(scenes[0]));
    assert_eq!(fields.missing(), SceneField::ALL.to_vec(), "还没填过：四格都缺");
}

#[test]
fn saving_trims_and_overwrites_the_whole_row() {
    let (_dir, mut store, _work, scenes) = seed();
    let target = scenes[0];

    let saved = store
        .save_scene_fields(
            &SceneFields {
                node_id: target,
                pov: "  林昭  ".to_string(),
                goal: "拿到账本".to_string(),
                conflict: "  \n ".to_string(),
                outcome: "被认出来".to_string(),
            },
            "test",
        )
        .unwrap();
    assert_eq!(saved.pov, "林昭", "存下来的是修剪过的");
    assert_eq!(saved.conflict, "");
    assert_eq!(saved.missing(), vec![SceneField::Conflict], "只有冲突那一格还空着");

    // 整行覆盖：第二次传空就把第一次填的清掉
    let cleared = store.save_scene_fields(&SceneFields::empty(target), "test").unwrap();
    assert_eq!(cleared, SceneFields::empty(target));
    assert_eq!(store.scene_fields(target).unwrap().missing().len(), 4);
}

#[test]
fn listing_a_works_scenes_carries_the_fields_in_tree_order() {
    let (_dir, mut store, work, scenes) = seed();
    store
        .save_scene_fields(
            &SceneFields {
                node_id: scenes[1],
                pov: "陆文".to_string(),
                goal: String::new(),
                conflict: String::new(),
                outcome: String::new(),
            },
            "test",
        )
        .unwrap();

    let listed = store.scene_cards_of_work(work).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].0, scenes[0], "按树里的顺序：开场在前");
    assert_eq!(listed[0].1, "开场");
    assert!(!listed[0].2.is_complete(), "开场那张还没填");
    assert_eq!(listed[1].0, scenes[1]);
    assert_eq!(listed[1].2.pov, "陆文");
    assert_eq!(listed[1].2.missing().len(), 3, "只填了视角那一格");

    // 软删掉的场景卡不再列出来（它的四格跟着走）
    store.soft_delete_node(scenes[0]).unwrap();
    let after = store.scene_cards_of_work(work).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].0, scenes[1]);
}
