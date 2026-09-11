//! 边写边存的**保险丝**验收：指纹读回校验、紧急快照、失败不留半截、编辑目标引导。
//!
//! 这些能力是"存了没有 / 存的还是不是我手上这份"的凭据——出错时它们决定
//! 「作者的字还在不在」，所以每条都要有测试盯着。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;
use yanmo_core::text::content_hash;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 直接改库，模拟"别的写入者 / 外部工具"动过这份正文。
fn overwrite_behind_the_store(store: &Store, node_id: i64, foreign_body: &str) {
    store
        .conn()
        .execute(
            "UPDATE node_contents SET body = ?1, content_hash = ?2 WHERE node_id = ?3",
            rusqlite::params![foreign_body, content_hash(foreign_body), node_id],
        )
        .unwrap();
}

fn snapshot_rows(store: &Store, node_id: i64) -> Vec<(String, String)> {
    let mut stmt = store
        .conn()
        .prepare("SELECT body, reason FROM snapshots WHERE node_id = ?1 ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map(rusqlite::params![node_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn fingerprint_reports_what_is_actually_stored() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;

    assert_eq!(store.body_fingerprint(node).unwrap(), "", "还没写过正文时没有指纹");

    store.write_body(node, "第一版正文。").unwrap();
    assert_eq!(store.body_fingerprint(node).unwrap(), content_hash("第一版正文。"));

    // 外部改动 → 指纹必须跟着变，否则"读回校验"就是摆设
    overwrite_behind_the_store(&store, node, "别人写进来的正文。");
    assert_ne!(store.body_fingerprint(node).unwrap(), content_hash("第一版正文。"));
    assert_eq!(store.body_fingerprint(node).unwrap(), content_hash("别人写进来的正文。"));
}

#[test]
fn emergency_snapshot_keeps_memory_text_then_restores_the_database() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(node, "我手上这份正文。").unwrap();

    // 库里被改成别的（模拟写入丢失 / 被外部改动）
    overwrite_behind_the_store(&store, node, "库里的另一份正文。");
    assert_ne!(store.body_fingerprint(node).unwrap(), content_hash("我手上这份正文。"));

    let stats = store.emergency_snapshot(node, "我手上这份正文。", "desync").unwrap();
    assert_eq!(stats.char_count, yanmo_core::text::count_chars("我手上这份正文。"));

    // ① 内存态先被留成快照（先保险）
    assert_eq!(
        snapshot_rows(&store, node),
        vec![("我手上这份正文。".to_string(), "desync".to_string())]
    );
    // ② 库里再被改回内存态（后修复）
    assert_eq!(store.read_body(node).unwrap(), "我手上这份正文。");
    assert_eq!(store.body_fingerprint(node).unwrap(), content_hash("我手上这份正文。"));

    // ③ 这次抢救本身也要留痕
    let ops: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM op_log WHERE entity = 'snapshots' AND op = 'emergency'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ops, 1);

    // 抢救之后再存同一份内容就是空操作（不会又写一次）
    let before: i64 = store.conn().query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0)).unwrap();
    store.emergency_snapshot(node, "我手上这份正文。", "desync").unwrap();
    let after: i64 = store.conn().query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0)).unwrap();
    assert_eq!(after, before + 1, "每次抢救都该留一份快照，哪怕正文本身没变");
}

#[test]
fn failed_write_never_leaves_half_a_body() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(node, "原来的正文。").unwrap();

    store.soft_delete_node(node).unwrap();
    assert!(store.write_body(node, "往回收站里写。").is_err());
    assert!(store.emergency_snapshot(node, "抢救已删节点。", "desync").is_err());
    assert_eq!(store.read_body(node).unwrap(), "原来的正文。", "失败的写入不得改动库里的正文");
    assert!(snapshot_rows(&store, node).is_empty(), "失败就不该留下快照垃圾");
}

#[test]
fn bootstrap_gives_something_to_write_on_first_run() {
    let (_dir, mut store) = fresh();
    let target = store.ensure_editor_target().unwrap();
    assert_eq!(store.list_works().unwrap().len(), 1, "首次运行应当只有一篇默认作品");
    let node = store
        .list_nodes(target.work_id)
        .unwrap()
        .into_iter()
        .find(|n| n.id == target.node_id)
        .expect("引导出来的节点必须真实存在");
    assert!(node.kind.holds_body(), "引导的目标必须是能落正文的节点");

    // 再引导一次：复用同一章，不再新建作品
    let again = store.ensure_editor_target().unwrap();
    assert_eq!(again.node_id, target.node_id);
    assert_eq!(store.list_works().unwrap().len(), 1);

    // 写完字以后，重新打开拿到的还是同一章，且正文还在
    store.write_body(target.node_id, "开篇第一句。").unwrap();
    let reopened = store.ensure_editor_target().unwrap();
    assert_eq!(reopened.node_id, target.node_id);
    assert_eq!(store.read_body(reopened.node_id).unwrap(), "开篇第一句。");
}

#[test]
fn bootstrap_falls_back_to_a_chapter_when_only_containers_exist() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    assert_eq!(store.list_nodes(work.id).unwrap().len(), 1, "长篇默认只有一卷");

    let target = store.ensure_editor_target().unwrap();
    let nodes = store.list_nodes(work.id).unwrap();
    assert_eq!(nodes.len(), 2, "只有容器时应当补一章出来");
    let chapter = nodes.iter().find(|n| n.id == target.node_id).unwrap();
    assert_eq!(chapter.kind, NodeKind::Chapter);
    assert_eq!(chapter.parent_id, Some(nodes[0].id), "新章应当挂在卷下");

    // 已经能落正文的作品：直接用它，不再新建
    let again = store.ensure_editor_target().unwrap();
    assert_eq!(again.node_id, target.node_id);
    assert_eq!(store.list_nodes(work.id).unwrap().len(), 2);
}
