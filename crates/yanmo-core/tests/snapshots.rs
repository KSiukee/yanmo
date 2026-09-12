//! 版本快照验收：**滚动保留有边界，回滚有保险，手动留的版本谁也删不掉**。
//!
//! 四条判据：
//! 1. 列表给的是摘要（新的在前），正文按需再拉；
//! 2. 自动快照滚动保留 N 份，手动留的版本（pinned）一份不动；
//! 3. 回滚**先把当前这一版留底**，再把正文改回去，字数与检索索引一起跟上；
//! 4. 快照不存在 / 节点已进回收站时明确报错，且不留垃圾。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{Store, AUTO_SNAPSHOTS_KEPT};
use yanmo_core::text;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书：一卷两章（都有正文）。
fn book(store: &mut Store, title: &str) -> (i64, i64, Vec<i64>) {
    let work = store.create_work(WorkKind::Novel, title).unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for name in ["第一章", "第二章"] {
        let id = store.create_node(work.id, Some(volume), NodeKind::Chapter, name).unwrap();
        store.write_body(id, &format!("{name}的正文。")).unwrap();
        chapters.push(id);
    }
    (work.id, volume, chapters)
}

/// 写一版正文并留一条自动快照（模拟"改一次 + 关一次窗"）。
fn revise(store: &mut Store, node_id: i64, body: &str) {
    store.write_body(node_id, body).unwrap();
    assert!(store.snapshot_if_changed(node_id, "close").unwrap(), "内容变了就该留快照：{body}");
}

fn count(store: &Store, sql: &str, id: i64) -> i64 {
    store.conn().query_row(sql, [id], |r| r.get(0)).unwrap()
}

#[test]
fn lists_newest_first_as_summaries() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store, "长夜");
    let node = chapters[0];

    revise(&mut store, node, "第一版。");
    revise(&mut store, node, "第二版。");

    let list = store.list_snapshots(node).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].reason, "close");
    assert_eq!(list[0].char_count, text::count_chars("第二版。"));
    assert!(list[0].created_at >= list[1].created_at, "新的必须排在前面");
    assert!(!list[0].pinned, "自动快照不是手动版本");
    // 摘要里没有正文：正文要单独拉
    assert_eq!(store.snapshot_body(list[0].id).unwrap(), "第二版。");
    assert_eq!(store.snapshot_body(list[1].id).unwrap(), "第一版。");
}

#[test]
fn rolling_keep_drops_old_autos_but_never_manual_versions() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store, "长夜");
    let node = chapters[0];

    // 还没到关窗就先手动留一版（手动版本与自动快照混在同一张表里）
    store.write_body(node, "值得留的一版。").unwrap();
    let pinned = store.keep_snapshot(node).unwrap().expect("第一次留版本应当落一条");

    for index in 0..25 {
        revise(&mut store, node, &format!("第{index}稿。"));
    }

    let list = store.list_snapshots(node).unwrap();
    let autos: Vec<_> = list.iter().filter(|entry| !entry.pinned).collect();
    assert_eq!(autos.len() as i64, AUTO_SNAPSHOTS_KEPT, "自动快照只留最近 N 份");
    assert_eq!(store.snapshot_body(autos[0].id).unwrap(), "第24稿。", "留下的必须是最新的那批");
    let kept = list.iter().find(|entry| entry.id == pinned).expect("手动版本不该被滚动删掉");
    assert!(kept.pinned);
    assert_eq!(store.snapshot_body(kept.id).unwrap(), "值得留的一版。");
}

#[test]
fn keeping_twice_without_a_change_does_not_pile_up() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store, "长夜");
    let node = chapters[0];
    store.write_body(node, "同一份。").unwrap();

    let first = store.keep_snapshot(node).unwrap().expect("第一次该留");
    assert_eq!(store.keep_snapshot(node).unwrap(), None, "内容没变就不该再堆一条");
    assert_eq!(store.list_snapshots(node).unwrap().len(), 1);
    assert!(store.list_snapshots(node).unwrap()[0].pinned);
    assert!(store.snapshot_body(first).is_ok());
}

#[test]
fn restore_keeps_the_replaced_version_and_brings_code_back_in_line() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store, "长夜");
    let node = chapters[0];

    revise(&mut store, node, "初稿的那一句话在这里。");
    let old = store.list_snapshots(node).unwrap()[0].id;
    // 这一版还没被留过快照（刚敲完，还没到关窗）：回滚要负责先替它留一份底
    store.write_body(node, "改坏了的第二版。").unwrap();

    let (node_id, body, stats) = store.restore_snapshot(old).unwrap();
    assert_eq!(node_id, node, "回执要带上这是哪一章");
    assert_eq!(body, "初稿的那一句话在这里。");
    assert_eq!(store.read_body(node).unwrap(), body, "正文要真的改回去");
    assert_eq!(stats.char_count, text::count_chars(&body));
    assert_eq!(
        count(&store, "SELECT word_count FROM nodes WHERE id = ?1", node),
        stats.word_count,
        "目录树的字数要跟着回滚"
    );

    // 回滚前那一版必须留了底（否则"回滚错了"就再也回不来了）
    let list = store.list_snapshots(node).unwrap();
    let backup = list.iter().find(|entry| entry.reason == "before_restore").expect("回滚前要先留一份");
    assert_eq!(store.snapshot_body(backup.id).unwrap(), "改坏了的第二版。");

    // 检索索引由触发器同步：回滚后搜得到旧稿、搜不到被换掉的那版
    let hits: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM node_fts WHERE node_fts MATCH '初稿的那一句话'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(hits, 1, "回滚后旧稿要搜得到");
    let gone: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM node_fts WHERE node_fts MATCH '改坏了的第二版'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(gone, 0, "被换掉的那版不该还留在索引里");
}

#[test]
fn dropping_a_version_touches_only_the_list() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store, "长夜");
    let node = chapters[0];
    store.write_body(node, "正文不动。").unwrap();
    let kept = store.keep_snapshot(node).unwrap().unwrap();

    store.drop_snapshot(kept).unwrap();
    assert!(store.list_snapshots(node).unwrap().is_empty());
    assert!(store.snapshot_body(kept).is_err(), "删掉的版本不该还能读出来");
    assert_eq!(store.read_body(node).unwrap(), "正文不动。", "删版本绝不碰正文");
    assert!(store.drop_snapshot(kept).is_err(), "删不存在的东西要明确报错");
}

#[test]
fn restoring_refuses_unknown_versions_and_trashed_nodes() {
    let (_dir, mut store) = fresh();
    let (_work, _volume, chapters) = book(&mut store, "长夜");
    let node = chapters[0];

    let missing = store.restore_snapshot(999_999).unwrap_err();
    assert_eq!(missing.code(), "snapshot.not_found");

    revise(&mut store, node, "进回收站之前写的一版。");
    let old = store.list_snapshots(node).unwrap()[0].id;
    store.soft_delete_node(node).unwrap();
    let before = count(&store, "SELECT COUNT(*) FROM snapshots WHERE node_id = ?1", node);
    assert!(store.restore_snapshot(old).is_err(), "回收站里的节点不给回滚");
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM snapshots WHERE node_id = ?1", node),
        before,
        "失败的动作为什么都不能留下快照垃圾"
    );
}
