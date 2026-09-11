//! 回收站验收：**删错了要能捞回来，捞回来的东西要看得见**。
//!
//! 三条判据：
//! 1. 软删的东西列得出来、恢复得回来（正文与结构都在）；
//! 2. 恢复一整段时，**它还留着的父级也一起回来**——否则捞出来了却挂在看不见的地方；
//! 3. 彻底删除只对回收站里的东西开放，且真的把正文与快照一并抹掉。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{Store, TrashKind};

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

/// 带一个参数的行数查询（测试里最常用的那种）。
fn count(store: &Store, sql: &str, id: i64) -> i64 {
    store.conn().query_row(sql, [id], |r| r.get(0)).unwrap()
}

/// 全表行数。
fn total(store: &Store, sql: &str) -> i64 {
    store.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn trash_lists_deleted_books_and_segments() {
    let (_dir, mut store) = fresh();
    let (gone, _volume, gone_chapters) = book(&mut store, "删掉的那本");
    let (alive, _volume2, alive_chapters) = book(&mut store, "还在的那本");

    store.soft_delete_work(gone).unwrap();
    store.soft_delete_node(alive_chapters[0]).unwrap();

    let trash = store.list_trash().unwrap();
    assert_eq!(trash.len(), 2, "一本书 + 一段：{trash:?}");

    let by_book = trash.iter().find(|e| e.kind == TrashKind::Work).unwrap();
    assert_eq!(by_book.id, gone);
    assert_eq!(by_book.title, "删掉的那本");
    assert_eq!(by_book.work_title, "删掉的那本", "整本书的条目用它自己的名字");
    assert_eq!(by_book.nodes, 3, "卷 + 两章都会跟着回来");

    let segment = trash.iter().find(|e| e.kind == TrashKind::Node).unwrap();
    assert_eq!(segment.title, "第一章");
    assert_eq!(segment.work_title, "还在的那本", "段条目要知道它属于哪本书");
    assert_eq!(segment.work_id, alive);
    assert_eq!(segment.nodes, 1);
    assert!(gone_chapters.iter().all(|id| *id != segment.id));
}

#[test]
fn restoring_a_book_brings_the_whole_book_back() {
    let (_dir, mut store) = fresh();
    let (work, _volume, chapters) = book(&mut store, "长夜");
    store.soft_delete_node(chapters[0]).unwrap(); // 先单独删了一章
    store.soft_delete_work(work).unwrap(); // 后来把整本书删了

    assert!(store.shelf().unwrap().is_empty(), "删掉的书不在书架上");
    assert_eq!(store.list_trash().unwrap().len(), 1, "只列整本书这一条");

    assert_eq!(store.restore_work(work).unwrap(), 1);
    assert_eq!(store.shelf().unwrap().len(), 1, "书回到了书架");
    assert_eq!(store.read_body(chapters[1]).unwrap(), "第二章的正文。", "正文完好");

    // ★ 单独删过的那一章留在回收站里：那是另一次删除，"恢复整本书"不该顺手撤销
    let trash = store.list_trash().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, chapters[0]);

    // 恢复它，书就完整了
    store.restore_node(chapters[0]).unwrap();
    assert!(store.list_trash().unwrap().is_empty());
    assert_eq!(store.chapter_neighbors(chapters[0]).unwrap().total, 2);
}

#[test]
fn restoring_a_segment_brings_back_its_subtree_and_its_parents() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = book(&mut store, "长夜");

    // 删整卷：回收站里只该出现"卷"这一条（章是跟着一起走的）
    store.soft_delete_node(volume).unwrap();
    let trash = store.list_trash().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, volume);
    assert_eq!(trash[0].nodes, 3, "卷 + 两章");
    assert!(store.list_nodes(work).unwrap().is_empty(), "整卷看不见了");

    let restored = store.restore_node(volume).unwrap();
    assert_eq!(restored, 3, "整棵子树一起回来");
    assert_eq!(store.list_nodes(work).unwrap().len(), 3);
    assert_eq!(store.read_body(chapters[0]).unwrap(), "第一章的正文。");

    // 另一种情形：先删一章，再删它所在的卷；按章恢复时**父链也要回来**
    store.soft_delete_node(volume).unwrap(); // 卷（此刻章已经是活的，一起被删）
    store.restore_node(volume).unwrap();
    store.soft_delete_node(chapters[0]).unwrap(); // 只删章
    store.soft_delete_node(volume).unwrap(); // 再删卷（章已经在回收站里了）
    store.restore_node(chapters[0]).unwrap(); // 直接捞那一章
    let nodes = store.list_nodes(work).unwrap();
    assert_eq!(nodes.len(), 2, "章回来了，**卷也一起回来**（否则它挂在看不见的父级下）");
    assert!(nodes.iter().any(|n| n.id == volume));
    assert_eq!(store.read_body(chapters[0]).unwrap(), "第一章的正文。");

    // ★ 只捞了那一章：同一次删除里被带走的**兄弟章留在回收站里**，
    //   而且因为它的父级已经活了，它自己会作为一条独立条目出现——想捞再捞
    let left = store.list_trash().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, chapters[1], "兄弟章还在回收站里等着");
    store.restore_node(chapters[1]).unwrap();
    assert!(store.list_trash().unwrap().is_empty());
    assert_eq!(store.list_nodes(work).unwrap().len(), 3);
}

#[test]
fn purge_only_works_on_the_trash_and_really_deletes() {
    let (_dir, mut store) = fresh();
    let (work, _volume, chapters) = book(&mut store, "长夜");

    // 活着的东西不许被"彻底删除"绕过回收站
    assert!(store.purge_node(chapters[0]).is_err(), "没进回收站就不给真删");
    assert!(store.purge_work(work).is_err());
    assert!(store.restore_node(chapters[0]).is_err());
    assert!(store.restore_work(work).is_err());
    assert!(store.purge_node(999_999).is_err());

    // 真删：正文与快照一起走
    store.write_body(chapters[0], "改过之后的正文。").unwrap();
    assert!(store.snapshot_if_changed(chapters[0], "manual").unwrap(), "先留一份旧稿");
    store.soft_delete_node(chapters[0]).unwrap();
    assert_eq!(store.purge_node(chapters[0]).unwrap(), 1);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM nodes WHERE id = ?1", chapters[0]), 0);
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM node_contents WHERE node_id = ?1", chapters[0]),
        0,
        "正文跟着走（外键级联）"
    );
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM snapshots WHERE node_id = ?1", chapters[0]),
        0,
        "快照也跟着走"
    );

    // 删整本书：书里所有节点一起走
    store.soft_delete_work(work).unwrap();
    assert_eq!(store.purge_work(work).unwrap(), 2, "剩下的两个节点");
    assert_eq!(total(&store, "SELECT COUNT(*) FROM works"), 0);
    assert_eq!(total(&store, "SELECT COUNT(*) FROM nodes"), 0);
}

#[test]
fn empty_trash_clears_everything_at_once() {
    let (_dir, mut store) = fresh();
    let (one, _v1, chapters_one) = book(&mut store, "第一本");
    let (two, _v2, _chapters_two) = book(&mut store, "第二本");

    store.soft_delete_work(one).unwrap();
    store.soft_delete_node(chapters_one[0]).unwrap(); // 书已经在回收站里：这条不单独出现
    store.soft_delete_work(two).unwrap();
    assert_eq!(store.list_trash().unwrap().len(), 2, "两本书各一条；书里的段不再重复列");

    assert_eq!(store.empty_trash().unwrap(), 2);
    assert!(store.list_trash().unwrap().is_empty());
    assert_eq!(total(&store, "SELECT COUNT(*) FROM works"), 0);
    assert_eq!(total(&store, "SELECT COUNT(*) FROM nodes"), 0);
    assert_eq!(total(&store, "SELECT COUNT(*) FROM node_contents"), 0);
}
