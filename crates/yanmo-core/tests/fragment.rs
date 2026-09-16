//! 创作流碎片验收：**碎片统一表 + 记 / 看 / 删 / 捞回**。
//!
//! 这一份盯五件事，每一件都能被机械地判：
//!
//! 1. **同一张表**：灵感 / 事件 / 口述都进 `fragments`，靠 `frag_kind` 分家——
//!    读回来种类、来源、锚点、时刻一样不少；
//! 2. **进得来也出得去**：随手记的每一种都能建、能读、能软删、能捞回；
//! 3. **不该建的建不成**：问题卡与答案归叩问那条线，从这里建当场被拒且**一个字节不写**；
//! 4. **删是软删**：删完列表与计数都不算它，但行还在（捞回就是抹掉时间戳）；
//! 5. **有据可查**：每次真写入都在 op-log 留一条（建 / 删 / 捞回各自的动词）。
//!
//! 判据不写死"哪几种"：可记的那一组直接取 [`FragmentKind::JOTTED`]，
//! 所以**将来加一种就会自动被这里验到**，不需要人来补测试。

use yanmo_core::error::codes;
use yanmo_core::model::{FragmentKind, WorkKind};
use yanmo_core::store::{NewFragment, Store, FRAGMENTS_PER_BOARD};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn new_fragment(work_id: i64, kind: FragmentKind, body: &str) -> NewFragment {
    NewFragment {
        work_id,
        kind,
        body: body.to_string(),
        source: "typed".to_string(),
        anchors: Vec::new(),
    }
}

/// op-log 里这条碎片上的操作（`(op, payload)`，按发生顺序）。
fn events_of(store: &Store, id: i64) -> Vec<(String, String)> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity='fragments' AND entity_id=?1 ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// 库里那一行还在不在（**软删的也算在**——删没删是那一列的事）。
fn row_count(store: &Store, id: i64) -> i64 {
    store
        .conn()
        .query_row("SELECT COUNT(*) FROM fragments WHERE id = ?1", [id], |r| r.get(0))
        .unwrap()
}

/// 可记的那几种**每一种都真的到得了**（没有"定义了却产不出来"的死值）。
#[test]
fn every_jottable_kind_is_reachable() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    for kind in FragmentKind::JOTTED {
        let id = store
            .create_fragment(&new_fragment(work.id, kind, "一句"), "test")
            .unwrap();
        let back = store.fragment(id).unwrap();
        assert_eq!(back.kind, kind, "建进去哪一种，读回来就得是那一种");
    }
}

#[test]
fn fragments_come_back_newest_first_and_are_counted_by_kind() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let idea = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "一个念头"), "test")
        .unwrap();
    let event = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Event, "他走进来"), "test")
        .unwrap();

    let all = store
        .fragments(work.id, &FragmentKind::JOTTED, FRAGMENTS_PER_BOARD)
        .unwrap();
    assert_eq!(all.len(), 2);
    // 新的在前：同一个毫秒里建的两条，按 id 大的在前（时间戳可能撞）
    assert_eq!(all[0].id, event, "最近记的在最前");
    assert_eq!(all[1].id, idea);

    // 计数与列表同一口径：问几种就答几种，都给到（哪怕 0）
    let counts = store.fragment_counts(work.id, &FragmentKind::JOTTED).unwrap();
    assert_eq!(counts.len(), FragmentKind::JOTTED.len());
    let idea_count = counts.iter().find(|c| c.kind == FragmentKind::Idea).unwrap();
    assert_eq!(idea_count.count, 1);
    let dictation = counts.iter().find(|c| c.kind == FragmentKind::Dictation).unwrap();
    assert_eq!(dictation.count, 0, "没记过的种类也要如实回 0");

    // 只问一种：另一种不混进来
    let only_event = store
        .fragments(work.id, &[FragmentKind::Event], FRAGMENTS_PER_BOARD)
        .unwrap();
    assert_eq!(only_event.len(), 1);
    assert_eq!(only_event[0].id, event);
}

#[test]
fn question_and_answer_kinds_are_refused_without_writing_anything() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    for kind in [FragmentKind::Question, FragmentKind::Answer] {
        let err = store
            .create_fragment(&new_fragment(work.id, kind, "不该从这儿建"), "test")
            .unwrap_err();
        assert_eq!(err.code(), codes::FRAGMENT_KIND_NOT_JOTTED);
    }
    // 一个字节都没写：没有任何碎片行
    let total: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM fragments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 0, "被拒的建卡不许留下半行");
}

#[test]
fn empty_body_and_unknown_source_are_refused() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    // 只有空白的正文：不写（连修剪后的空串也不许进库）
    let blank = new_fragment(work.id, FragmentKind::Idea, "   \n ");
    assert_eq!(
        store.create_fragment(&blank, "test").unwrap_err().code(),
        codes::FRAGMENT_BODY_EMPTY
    );

    // 认不出的输入方式：这与答案、灵感卡共用同一个闭集，不能悄悄收下
    let mut bad = new_fragment(work.id, FragmentKind::Idea, "一句");
    bad.source = "telepathy".to_string();
    assert_eq!(
        store.create_fragment(&bad, "test").unwrap_err().code(),
        codes::INPUT_SOURCE_UNKNOWN
    );

    let total: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM fragments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 0);

    // 正文两边空白会被修剪：库里存的是修剪过的那一句
    let id = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "  一句  "), "test")
        .unwrap();
    assert_eq!(store.fragment(id).unwrap().body, "一句");
}

#[test]
fn anchors_and_source_are_stored_and_read_back() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let mut item = new_fragment(work.id, FragmentKind::Event, "他走进来");
    item.source = "voice".to_string();
    item.anchors = vec!["chapter:12".to_string(), "chapter:13".to_string()];
    let id = store.create_fragment(&item, "test").unwrap();

    let back = store.fragment(id).unwrap();
    assert_eq!(back.source, "voice");
    assert_eq!(back.anchors, vec!["chapter:12", "chapter:13"]);

    // 锚点那一列被手改成坏 JSON 时按空数组读（只是展示，不该让整屏读不出来）
    store
        .conn()
        .execute("UPDATE fragments SET linked='不是 JSON' WHERE id=?1", [id])
        .unwrap();
    assert!(store.fragment(id).unwrap().anchors.is_empty());
}

#[test]
fn deleting_is_soft_and_restoring_brings_it_back() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "一个念头"), "test")
        .unwrap();

    store.delete_fragment(id, "test").unwrap();
    assert!(store.fragment(id).is_err(), "删掉的按'取一条'就是不存在");
    assert!(store
        .fragments(work.id, &FragmentKind::JOTTED, FRAGMENTS_PER_BOARD)
        .unwrap()
        .is_empty());
    let counts = store.fragment_counts(work.id, &FragmentKind::JOTTED).unwrap();
    assert_eq!(counts.iter().map(|c| c.count).sum::<usize>(), 0);
    assert_eq!(row_count(&store, id), 1, "软删：行还在，只是打了时间戳");

    let back = store.restore_fragment(id, "test").unwrap();
    assert_eq!(back.id, id);
    assert_eq!(
        store
            .fragments(work.id, &FragmentKind::JOTTED, FRAGMENTS_PER_BOARD)
            .unwrap()
            .len(),
        1
    );

    // 证据：建 / 删 / 捞回各留一条，动词分得清
    let ops: Vec<String> = events_of(&store, id).into_iter().map(|(op, _)| op).collect();
    assert_eq!(ops, vec!["create", "delete", "restore"]);
}

#[test]
fn restoring_a_live_fragment_is_a_no_op_without_extra_evidence() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "一个念头"), "test")
        .unwrap();

    // 重复点"撤销"：原样返回，不报错、也不留第二条 create/restore
    let again = store.restore_fragment(id, "test").unwrap();
    assert_eq!(again.id, id);
    let ops: Vec<String> = events_of(&store, id).into_iter().map(|(op, _)| op).collect();
    assert_eq!(ops, vec!["create"]);
}

#[test]
fn deleting_twice_and_deleting_a_stranger_are_reported() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "一个念头"), "test")
        .unwrap();

    store.delete_fragment(id, "test").unwrap();
    // 第二次删：它已经不在"取得到"那一条路上了——如实说"不存在"，不静默当成删成功
    assert_eq!(
        store.delete_fragment(id, "test").unwrap_err().code(),
        codes::FRAGMENT_NOT_FOUND
    );
    assert_eq!(
        store.restore_fragment(9999, "test").unwrap_err().code(),
        codes::FRAGMENT_NOT_FOUND
    );
}

#[test]
fn fragments_of_other_works_never_mix() {
    let (_dir, mut store) = fresh();
    let one = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let two = store.create_work(WorkKind::Novel, "白日").unwrap();
    store
        .create_fragment(&new_fragment(one.id, FragmentKind::Idea, "甲"), "test")
        .unwrap();
    let b = store
        .create_fragment(&new_fragment(two.id, FragmentKind::Idea, "乙"), "test")
        .unwrap();

    let listed = store
        .fragments(two.id, &FragmentKind::JOTTED, FRAGMENTS_PER_BOARD)
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, b);
    assert_eq!(listed[0].work_id, two.id);
}

#[test]
fn writing_into_a_trashed_work_is_refused() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    store.soft_delete_work(work.id).unwrap();
    let err = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "一句"), "test")
        .unwrap_err();
    assert_eq!(err.code(), codes::WORK_GONE);
}

#[test]
fn an_unknown_kind_in_the_db_is_reported_not_guessed() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_fragment(&new_fragment(work.id, FragmentKind::Idea, "一句"), "test")
        .unwrap();
    // 库里被手改成认不出的种类：如实报错，不猜一个"像那么回事"的
    store
        .conn()
        .execute("UPDATE fragments SET frag_kind='memo' WHERE id=?1", [id])
        .unwrap();
    assert_eq!(
        store.fragment(id).unwrap_err().code(),
        codes::UNKNOWN_FRAGMENT_KIND
    );
}
