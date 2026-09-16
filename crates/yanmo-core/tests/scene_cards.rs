//! 四格验收：**四格缺项是"值为空"，不是另一张布尔**。
//!
//! 这一份盯五件事：
//! 1. **凡承载正文的节点都有四格**（章 / 节 / 单篇 / 场景卡），卷没有——如实分开报；
//! 2. 没有那一行 = 四格全空（缺项检测要的就是这个）；
//! 3. 存了读回来一致（值修剪首尾空白），再存是整行覆盖；
//! 4. 按书列出"填过的"时**带上四格**（没填过的不列），顺序按树里的顺序；
//! 5. 软删的节点不再列出来。

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

/// 卷没有四格（它不承载正文）；**章有**——中文网文的习惯就是一章一行。
#[test]
fn only_nodes_that_hold_body_have_four_fields() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();

    assert_eq!(
        store.scene_fields(volume).unwrap_err().code(),
        codes::NODE_NO_FIELDS,
        "卷没有那四格"
    );
    assert_eq!(store.scene_fields(9999).unwrap_err().code(), codes::NODE_GONE, "节点不在是另一回事");

    // 章直接填四格：不必先建一张场景卡
    assert!(store.has_fields(chapter).unwrap());
    assert!(!store.has_fields(volume).unwrap());
    let saved = store
        .save_scene_fields(
            &SceneFields {
                node_id: chapter,
                pov: "陆文".to_string(),
                goal: "拿到账本".to_string(),
                conflict: String::new(),
                outcome: String::new(),
            },
            "test",
        )
        .unwrap();
    assert_eq!(saved.pov, "陆文", "章上的四格照样存得下");
    assert_eq!(store.scene_fields(chapter).unwrap(), saved);
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

    // 只列**填过的**：开场那张一个字都没填，不在这里（体检也不念它）
    let listed = store.nodes_with_fields(work).unwrap();
    assert_eq!(listed.len(), 1, "只填过对峙那一张");
    assert_eq!(listed[0].0, scenes[1]);
    assert_eq!(listed[0].1, "对峙");
    assert_eq!(listed[0].2.pov, "陆文");
    assert_eq!(listed[0].2.missing().len(), 3, "只填了视角那一格");

    // 软删掉的场景卡不再列出来（它的四格跟着走）
    store.soft_delete_node(scenes[1]).unwrap();
    assert!(store.nodes_with_fields(work).unwrap().is_empty());
}
