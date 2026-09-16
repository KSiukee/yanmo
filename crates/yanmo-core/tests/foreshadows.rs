//! 伏笔验收：**埋 → 收 / 不写了**，而且每一步都留痕、都能回头。
//!
//! 五件事：
//! 1. 记一条从「埋着」开始（正文不许空、书得在）；
//! 2. 三个态都到得了，而且都回得到「埋着」（收错了 / 改主意了，不该被堵死）；
//! 3. 非法边当场拒，一个字节不写；
//! 4. 收的时候记"收在哪一章"；回到「埋着」时收点清掉；
//! 5. 软删（行还在）、按状态筛、跨书不混。

use yanmo_core::error::codes;
use yanmo_core::model::{ForeshadowState, NewForeshadow, NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn new_foreshadow(work_id: i64, planted: Option<i64>) -> NewForeshadow {
    NewForeshadow {
        work_id,
        body: "老张的怀表".to_string(),
        planted_node: planted,
        note: String::new(),
    }
}

#[test]
fn a_foreshadow_starts_planted_and_can_be_collected() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let third = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第三章").unwrap();
    let twelfth = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第十二章").unwrap();

    let id = store
        .create_foreshadow(&new_foreshadow(work.id, Some(third)), "test")
        .unwrap();
    let planted = store.foreshadow(id).unwrap();
    assert_eq!(planted.state, ForeshadowState::Planted);
    assert_eq!(planted.planted_node, Some(third));
    assert_eq!(planted.collected_node, None);

    // 收了：带上收在哪一章
    let collected = store
        .move_foreshadow(id, ForeshadowState::Collected, Some(twelfth), "test")
        .unwrap();
    assert_eq!(collected.state, ForeshadowState::Collected);
    assert_eq!(collected.collected_node, Some(twelfth));

    // 收错了 / 改主意：回到「埋着」，收点跟着清掉
    let back = store
        .move_foreshadow(id, ForeshadowState::Planted, None, "test")
        .unwrap();
    assert_eq!(back.state, ForeshadowState::Planted);
    assert_eq!(back.collected_node, None, "回到埋着就该把收点清掉");
}

#[test]
fn every_state_is_reachable_but_illegal_edges_are_refused() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store.create_foreshadow(&new_foreshadow(work.id, None), "test").unwrap();

    // 「不写了」也是正经结局
    store.move_foreshadow(id, ForeshadowState::Dropped, None, "test").unwrap();
    assert_eq!(store.foreshadow(id).unwrap().state, ForeshadowState::Dropped);

    // 收到了「不写了」之间不许直接跳（先回埋着，一次说清一件事）
    let err = store
        .move_foreshadow(id, ForeshadowState::Collected, None, "test")
        .unwrap_err();
    assert_eq!(err.code(), codes::FORESHADOW_ILLEGAL_TRANSITION);
    // 同一个态也不算一次迁移
    assert_eq!(
        store.move_foreshadow(id, ForeshadowState::Dropped, None, "test").unwrap_err().code(),
        codes::FORESHADOW_ILLEGAL_TRANSITION
    );
    assert_eq!(store.foreshadow(id).unwrap().state, ForeshadowState::Dropped, "被拒之后没动过");
}

#[test]
fn empty_body_and_unknown_state_are_refused() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let mut draft = new_foreshadow(work.id, None);
    draft.body = "   \n ".to_string();
    assert_eq!(
        store.create_foreshadow(&draft, "test").unwrap_err().code(),
        codes::FORESHADOW_BODY_EMPTY
    );

    let id = store.create_foreshadow(&new_foreshadow(work.id, None), "test").unwrap();
    store
        .conn()
        .execute("UPDATE foreshadows SET state='maybe' WHERE id=?1", [id])
        .unwrap();
    assert_eq!(
        store.foreshadow(id).unwrap_err().code(),
        codes::UNKNOWN_FORESHADOW_STATE,
        "库里认不出的状态如实报错，不猜一个像那么回事的"
    );
}

#[test]
fn listing_is_per_work_and_planted_ones_come_first() {
    let (_dir, mut store) = fresh();
    let one = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let two = store.create_work(WorkKind::Novel, "白日").unwrap();

    let a = store.create_foreshadow(&new_foreshadow(one.id, None), "test").unwrap();
    let b = store.create_foreshadow(&new_foreshadow(one.id, None), "test").unwrap();
    store.create_foreshadow(&new_foreshadow(two.id, None), "test").unwrap();
    store.move_foreshadow(a, ForeshadowState::Collected, None, "test").unwrap();

    let all = store.foreshadows(one.id, None).unwrap();
    assert_eq!(all.len(), 2, "只列这本书的");
    assert_eq!(all[0].id, b, "还埋着的排前面（要看的先看得见）");
    let planted = store.foreshadows(one.id, Some(ForeshadowState::Planted)).unwrap();
    assert_eq!(planted.len(), 1);
    assert_eq!(planted[0].id, b);
}

#[test]
fn editing_keeps_the_state_and_deleting_is_soft() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();
    let id = store.create_foreshadow(&new_foreshadow(work.id, None), "test").unwrap();

    // 改正文与埋点：状态一个字节不动（状态走 move）
    let edited = store
        .update_foreshadow(id, "  老张的怀表（改成：怀表里的照片）  ", Some(chapter), "先放着", "test")
        .unwrap();
    assert_eq!(edited.body, "老张的怀表（改成：怀表里的照片）");
    assert_eq!(edited.planted_node, Some(chapter));
    assert_eq!(edited.state, ForeshadowState::Planted);
    assert_eq!(edited.note, "先放着");

    // 软删：取不到、列不到，行还在
    let gone = store.delete_foreshadow(id, "test").unwrap();
    assert_eq!(gone.id, id);
    assert!(store.foreshadow(id).is_err());
    assert!(store.foreshadows(work.id, None).unwrap().is_empty());
    let rows: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM foreshadows WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "软删：行还在，只是打了时间戳");
}

/// 证据：记 / 改 / 走一步 / 删各留一条 op-log，走一步那条写清从哪到哪。
#[test]
fn every_step_leaves_a_trace() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let third = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第三章").unwrap();
    let ninth = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第九章").unwrap();

    let id = store.create_foreshadow(&new_foreshadow(work.id, Some(third)), "test").unwrap();
    store.update_foreshadow(id, "改一句", Some(third), "", "test").unwrap();
    store.move_foreshadow(id, ForeshadowState::Collected, Some(ninth), "test").unwrap();
    store.delete_foreshadow(id, "test").unwrap();

    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity='foreshadows' AND entity_id=?1 ORDER BY seq")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let ops: Vec<&str> = rows.iter().map(|(op, _)| op.as_str()).collect();
    assert_eq!(ops, vec!["create", "update", "move", "delete"]);
    let moved = &rows[2].1;
    assert!(moved.contains("\"from\":\"planted\""), "从哪来要写清：{moved}");
    assert!(moved.contains("\"to\":\"collected\""), "到哪去要写清：{moved}");
    assert!(moved.contains(&format!("\"collected_node\":{ninth}")), "收在哪一章也留着：{moved}");
}

/// 脏锚点：**核心先说人话**，别让"数据库约束失败"露到界面上。
#[test]
fn a_bad_anchor_is_refused_with_a_readable_code() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let other = store.create_work(WorkKind::Novel, "白日").unwrap();
    let other_chapter = store
        .create_node(other.id, None, NodeKind::Chapter, "别人家的一章")
        .unwrap();

    // 不存在的节点
    assert_eq!(
        store
            .create_foreshadow(&new_foreshadow(work.id, Some(9999)), "test")
            .unwrap_err()
            .code(),
        codes::FORESHADOW_ANCHOR_INVALID
    );
    // 别的书里的节点
    assert_eq!(
        store
            .create_foreshadow(&new_foreshadow(work.id, Some(other_chapter)), "test")
            .unwrap_err()
            .code(),
        codes::FORESHADOW_ANCHOR_INVALID
    );

    // 收点同样得对得上（错的不写入，状态也不动）
    let id = store.create_foreshadow(&new_foreshadow(work.id, None), "test").unwrap();
    assert_eq!(
        store
            .move_foreshadow(id, ForeshadowState::Collected, Some(other_chapter), "test")
            .unwrap_err()
            .code(),
        codes::FORESHADOW_ANCHOR_INVALID
    );
    assert_eq!(store.foreshadow(id).unwrap().state, ForeshadowState::Planted);

    // 已删除的节点也不行（伏笔挂在删掉的那一章上没有意义）
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "要删的一章").unwrap();
    store.soft_delete_node(chapter).unwrap();
    assert_eq!(
        store
            .update_foreshadow(id, "改一句", Some(chapter), "", "test")
            .unwrap_err()
            .code(),
        codes::FORESHADOW_ANCHOR_INVALID
    );
}
