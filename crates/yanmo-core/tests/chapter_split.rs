//! 拆章验收：**切点处不丢不重、拆完能撤销、章名与位置对得上**。
//!
//! 这一条与其余写操作的分寸不同：它会**改正文**。所以这里盯的是三件事——
//! ① 拼回来必须与原文**逐字相同**（不是"看着差不多"）；
//! ② 拆完那一刻真的留了底、撤销真能把两章合回原来那一章；
//! ③ 不该拆的东西（卷、场景卡）当场拒，切点不合法（开头 / 末尾）也当场拒。

use yanmo_core::error::codes;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书 + 一个卷 + 三章；返回 `(work_id, 卷 id, 三个章 id)`。
fn book(store: &mut Store) -> (i64, i64, Vec<i64>) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for title in ["第一章", "第二章", "第三章"] {
        chapters.push(store.create_node(work.id, Some(volume), NodeKind::Chapter, title).unwrap());
    }
    (work.id, volume, chapters)
}

/// 三段正文（段与段之间一个空行）。
const THREE: &str = "第一段。\n\n第二段。\n\n第三段。";

#[test]
fn the_two_halves_put_back_together_are_the_original_text() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store);
    store.write_body(chapters[0], THREE).unwrap();

    let receipt = store.split_chapter(chapters[0], 1, "").unwrap();

    let head = store.read_body(receipt.old_node_id).unwrap();
    let tail = store.read_body(receipt.new_node_id).unwrap();
    assert_eq!(head, "第一段。");
    assert_eq!(tail, "第二段。\n\n第三段。");
    // 逐字拼回来 == 原文（这就是"不丢不重"的判据）
    assert_eq!(format!("{head}\n\n{tail}"), THREE);
    assert_eq!(receipt.kept_chars, 4);
    assert_eq!(receipt.moved_chars, 8);
}

#[test]
fn the_new_chapter_sits_right_after_the_old_one() {
    let (_dir, mut store) = fresh();
    let (work, _volume, chapters) = book(&mut store);
    store.write_body(chapters[0], THREE).unwrap();

    let receipt = store.split_chapter(chapters[0], 1, "").unwrap();

    let ids: Vec<i64> = store
        .list_nodes(work)
        .unwrap()
        .into_iter()
        .filter(|node| node.kind == NodeKind::Chapter)
        .map(|node| node.id)
        .collect();
    assert_eq!(
        ids,
        vec![chapters[0], receipt.new_node_id, chapters[1], chapters[2]],
        "新章必须紧挨着原章之后"
    );
    // 标题留空 ⇒ 核心按同层序号取名，而且回执给的是**渲染后**的那一份——
    // 真机走查逮到过：库里存的是模板 `第{$N}章`，条上直接把模板原文念了出来。
    assert!(!receipt.new_title.contains("{$N}"), "回执里的章名必须是渲染后的：{receipt:?}");
    assert_eq!(receipt.new_title, store.rendered_title(receipt.new_node_id).unwrap());
    assert!(!receipt.new_title.trim().is_empty());
}

#[test]
fn only_the_blank_line_at_the_cut_is_touched() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store);
    // 段内的硬换行 + 行尾空白：一个字都不许被"顺手规范化"掉
    let body = "头一段。\n\n  带硬换行的那段\n还接着写  \n\n尾段。";
    store.write_body(chapters[0], body).unwrap();

    let receipt = store.split_chapter(chapters[0], 1, "").unwrap();
    let head = store.read_body(receipt.old_node_id).unwrap();
    let tail = store.read_body(receipt.new_node_id).unwrap();

    assert_eq!(head, "头一段。");
    assert_eq!(tail, "  带硬换行的那段\n还接着写  \n\n尾段。");
    assert_eq!(format!("{head}\n\n{tail}"), body, "除了切点那一处空行，别的字节一个都不许动");
}

#[test]
fn an_impossible_cut_point_is_refused_on_the_spot() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store);
    store.write_body(chapters[0], THREE).unwrap();

    // 切在第一段之前 ⇒ 前半会是空的
    let error = store.split_chapter(chapters[0], 0, "").unwrap_err();
    assert_eq!(error.code(), codes::CHAPTER_SPLIT_POINT_INVALID);
    // 切在最后一段之后 ⇒ 后半会是空的（拆了等于没拆）
    let error = store.split_chapter(chapters[0], 3, "").unwrap_err();
    assert_eq!(error.code(), codes::CHAPTER_SPLIT_POINT_INVALID);
    // 被拒时**正文一字不动**
    assert_eq!(store.read_body(chapters[0]).unwrap(), THREE);
}

#[test]
fn a_volume_or_a_scene_card_cannot_be_split() {
    let (_dir, mut store) = fresh();
    let (work, volume, _chapters) = book(&mut store);
    let scene = store
        .create_node(work, Some(volume), NodeKind::Scene, "开场")
        .unwrap();
    store.write_body(scene, THREE).unwrap();

    for node_id in [volume, scene] {
        let error = store.split_chapter(node_id, 1, "").unwrap_err();
        assert_eq!(
            error.code(),
            codes::CHAPTER_SPLIT_NODE_UNSUPPORTED,
            "卷与场景卡都不该被拆（节点 {node_id}）"
        );
    }
    assert_eq!(store.read_body(scene).unwrap(), THREE, "被拒时正文一字不动");
}

#[test]
fn split_leaves_a_way_back_and_undo_rejoins_the_two_halves() {
    let (_dir, mut store) = fresh();
    let (work, _volume, chapters) = book(&mut store);
    store.write_body(chapters[0], THREE).unwrap();

    let receipt = store.split_chapter(chapters[0], 1, "分出来的这一章").unwrap();
    // 拆之前那一份底在（「版本」里看得见，撤销也认它）
    let snapshots = store.list_snapshots(chapters[0]).unwrap();
    assert_eq!(snapshots.first().map(|s| s.reason.as_str()), Some("before_split"));
    assert_eq!(snapshots.first().map(|s| s.id), Some(receipt.snapshot_id));

    store
        .undo_split(receipt.old_node_id, receipt.new_node_id, receipt.snapshot_id)
        .unwrap();

    assert_eq!(store.read_body(chapters[0]).unwrap(), THREE, "两半合回原样");
    assert!(!store
        .list_nodes(work)
        .unwrap()
        .iter()
        .any(|node| node.id == receipt.new_node_id), "新章不再挂在树上");
    // **进回收站，不是销毁**：作者在新章里写的字还捞得回来
    assert!(store
        .list_trash()
        .unwrap()
        .iter()
        .any(|entry| entry.id == receipt.new_node_id));
}

#[test]
fn undo_refuses_a_snapshot_that_belongs_to_another_chapter() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store);
    store.write_body(chapters[0], THREE).unwrap();
    store.write_body(chapters[1], "别人的正文。").unwrap();

    let receipt = store.split_chapter(chapters[0], 1, "").unwrap();
    // 拿**另一章**的底来撤销 ⇒ 当场拒（否则等于把两章的字搅在一起）
    let foreign = store.keep_snapshot(chapters[1]).unwrap().unwrap();
    let error = store
        .undo_split(receipt.old_node_id, receipt.new_node_id, foreign)
        .unwrap_err();
    assert_eq!(error.code(), codes::SNAPSHOT_NOT_FOUND);
    // 拒绝之后什么都没变：原章还是前半，新章还在
    assert_eq!(store.read_body(receipt.old_node_id).unwrap(), "第一段。");
    assert_eq!(store.read_body(receipt.new_node_id).unwrap(), "第二段。\n\n第三段。");
}
