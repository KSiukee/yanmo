//! 灵感验收：**引力排出来 + 采纳到四个落点 + 丢弃能捞回**。
//!
//! 七条判据（每条钉一件真会发生的事）：
//! 1. 候选**只有作者随手记的那几种**——问题卡与答案归叩问那条线，不进这个池子；
//! 2. 当前这一章有关的排前面；**没打开章时不惩罚任何人**；
//! 3. 采纳到**章纲**：原有的一句话留着，新的接在后面；同一句落两回不重复接；
//! 4. 采纳到**场景卡**：这一章下面多一张，正文就是这条灵感；
//! 5. 采纳到**事件池**：多一条事件、`derived_from` 指回它，**原文一个字不动**；
//! 6. 采纳会**用掉一次**（`used_count` +1、引力下降）；丢弃**不记账**；
//! 7. 丢弃是**软删**：捞回之后它还在，而且还能再被采纳。

use yanmo_core::error::codes;
use yanmo_core::model::{FragmentKind, NewQuestionCard, NodeKind, WorkKind};
use yanmo_core::store::{NewFragment, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书 + 一卷 + 两章（多数用例都要这套骨架）。
fn book(store: &mut Store) -> (i64, i64, i64) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap().id;
    let volume = store.create_node(work, None, NodeKind::Volume, "第一卷").unwrap();
    let first = store.create_node(work, Some(volume), NodeKind::Chapter, "第一章").unwrap();
    let second = store.create_node(work, Some(volume), NodeKind::Chapter, "第二章").unwrap();
    (work, first, second)
}

fn jot(store: &mut Store, work: i64, body: &str, anchors: Vec<String>) -> i64 {
    store
        .create_fragment(
            &NewFragment {
                work_id: work,
                kind: FragmentKind::Idea,
                body: body.to_string(),
                source: "typed".to_string(),
                anchors,
            },
            "test",
        )
        .unwrap()
}

/// 把一条碎片的"记下的时刻"挪到 `days_ago` 天前——排序要看它
/// （同一毫秒里记下的两条分不出先后，测试不能靠运气）。
fn age(store: &Store, id: i64, days_ago: i64) {
    let at = yanmo_core::time::now_millis() - days_ago * 86_400_000;
    store
        .conn()
        .execute("UPDATE fragments SET created_at = ?1 WHERE id = ?2", [at, id])
        .unwrap();
}

fn used(store: &Store, id: i64) -> i64 {
    store
        .conn()
        .query_row("SELECT used_count FROM fragments WHERE id = ?1", [id], |r| r.get(0))
        .unwrap()
}

fn summary(store: &Store, node: i64) -> String {
    store
        .conn()
        .query_row("SELECT summary FROM nodes WHERE id = ?1", [node], |r| r.get(0))
        .unwrap()
}

fn order(store: &Store, work: i64, node: Option<i64>) -> Vec<i64> {
    store
        .spark_candidates(work, node, 10)
        .unwrap()
        .iter()
        .map(|s| s.id)
        .collect()
}

fn gravity_of(store: &Store, work: i64, id: i64) -> f64 {
    store
        .spark_candidates(work, None, 50)
        .unwrap()
        .iter()
        .find(|s| s.id == id)
        .expect("这条该在候选里")
        .gravity
        .total
}

#[test]
fn only_jotted_fragments_are_candidates() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    jot(&mut store, work, "一句灵感", vec![]);
    // 问题卡与答案归叩问那条线：它们有自己的处置面板，不进灵感这个池子
    store
        .create_question_card(&NewQuestionCard {
            work_id: work,
            body: "这一章要不要收线？".to_string(),
            source: "core".to_string(),
            template_key: String::new(),
            importance: 1.0,
            linked: vec![format!("chapter:{first}")],
            derived_from: None,
            auto_derived: false,
        })
        .unwrap();

    let sparks = store.spark_candidates(work, Some(first), 10).unwrap();
    assert_eq!(sparks.len(), 1, "候选里只该有随手记的那一条");
    assert_eq!(sparks[0].body, "一句灵感");
    assert_eq!(sparks[0].kind, FragmentKind::Idea);
}

#[test]
fn the_current_chapter_pulls_related_sparks_up() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    // 两条一样新、一样重要、都没被采纳过：差别只在"跟不跟当前这一章有关"
    let elsewhere = jot(&mut store, work, "落在第一章的念头", vec![format!("chapter:{first}")]);
    let here = jot(&mut store, work, "就在这一章的念头", vec![format!("chapter:{second}")]);

    assert_eq!(order(&store, work, Some(second)), vec![here, elsewhere]);
    assert_eq!(order(&store, work, Some(first)), vec![elsewhere, here], "换一章就换过来");

    // 没打开章（作者只是翻看）：两条都在——**不因为"不知道他在哪"就把谁压下去**
    let both = order(&store, work, None);
    assert_eq!(both.len(), 2);
    assert!(
        both.iter().all(|id| store
            .spark_candidates(work, None, 10)
            .unwrap()
            .iter()
            .any(|s| s.id == *id)),
        "带着焦点算出来的候选不该凭空多出或丢掉谁"
    );
}

#[test]
fn aging_sparks_fall_behind_fresh_ones() {
    let (_dir, mut store) = fresh();
    let (work, _first, _second) = book(&mut store);
    let old = jot(&mut store, work, "上个月记的", vec![]);
    age(&store, old, 30);
    let fresh = jot(&mut store, work, "刚记的", vec![]);

    assert_eq!(order(&store, work, None), vec![fresh, old], "新鲜的排前面");
}

#[test]
fn adopting_to_outline_merges_into_the_one_line() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    // 这一章已经有一句作者自己写的话：合并**不是覆盖**
    store
        .conn()
        .execute(
            "UPDATE nodes SET summary = ?1 WHERE id = ?2",
            ["原本的一句话".to_string(), first.to_string()],
        )
        .unwrap();
    let spark = jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{first}")]);

    let receipt = store.adopt_spark(spark, "outline", Some(first), "", "test").unwrap();
    assert_eq!(receipt.outline.as_deref(), Some("原本的一句话；雨夜那一场要收线"));
    assert_eq!(summary(&store, first), "原本的一句话；雨夜那一场要收线");
    assert_eq!(receipt.text, "", "落章纲不动编辑器");
    assert_eq!(used(&store, spark), 1, "采纳一次记一笔");

    // 同一句再落一次：章纲里不该出现两遍，但那又是一笔账（作者确实又采纳了一回）
    store.adopt_spark(spark, "outline", Some(first), "", "test").unwrap();
    assert_eq!(summary(&store, first), "原本的一句话；雨夜那一场要收线");
    assert_eq!(used(&store, spark), 2);
}

#[test]
fn adopting_to_scene_card_creates_one_under_this_chapter() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let spark = jot(&mut store, work, "雨夜，城门下的一场对峙", vec![format!("chapter:{first}")]);

    let receipt = store.adopt_spark(spark, "scene", Some(first), "雨夜对峙", "test").unwrap();
    let scene = receipt.scene_id.expect("该新建一张场景卡");
    let (parent, kind): (Option<i64>, String) = store
        .conn()
        .query_row("SELECT parent_id, node_kind FROM nodes WHERE id = ?1", [scene], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(parent, Some(first), "挂在刚才那一章下面");
    assert_eq!(kind, NodeKind::Scene.as_str());
    assert_eq!(store.read_body(scene).unwrap(), "雨夜，城门下的一场对峙");
    assert_eq!(used(&store, spark), 1);
}

#[test]
fn adopting_to_event_pool_keeps_the_original_and_points_back() {
    let (_dir, mut store) = fresh();
    let (work, _first, _second) = book(&mut store);
    let spark = jot(&mut store, work, "主角在第三章才知道真相", vec![]);

    let receipt = store.adopt_spark(spark, "event", None, "", "test").unwrap();
    let event = receipt.event_id.expect("该新建一条事件");
    let (kind, body, from): (String, String, Option<i64>) = store
        .conn()
        .query_row(
            "SELECT frag_kind, body, derived_from FROM fragments WHERE id = ?1",
            [event],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(kind, FragmentKind::Event.as_str());
    assert_eq!(body, "主角在第三章才知道真相");
    assert_eq!(from, Some(spark), "溯源指回这条灵感");
    // 原文一个字不动（升成事件是"另起一条"，不是搬家）
    assert_eq!(store.fragment(spark).unwrap().body, "主角在第三章才知道真相");

    // 已经是事件的，不该再"采纳到事件池"（那只会长出一条重复的事件）
    let err = store.adopt_spark(event, "event", None, "", "test").unwrap_err();
    assert_eq!(err.code(), codes::SPARK_ALREADY_EVENT);
}

#[test]
fn adopting_uses_it_up_so_it_stops_camping_on_top() {
    let (_dir, mut store) = fresh();
    let (work, _first, _second) = book(&mut store);
    let taken = jot(&mut store, work, "被采纳过的", vec![]);
    let untouched = jot(&mut store, work, "还没用过的", vec![]);
    // 让被采纳的那条**更新**：单靠新鲜度它本该在前
    age(&store, taken, 0);
    age(&store, untouched, 1);
    assert_eq!(order(&store, work, None), vec![taken, untouched]);

    let before = gravity_of(&store, work, taken);
    store.adopt_spark(taken, "body", None, "", "test").unwrap();
    let after = gravity_of(&store, work, taken);
    assert!(after < before, "采纳一次要掉一截：{before} → {after}");
    assert_eq!(used(&store, taken), 1);
    assert_eq!(
        order(&store, work, None),
        vec![untouched, taken],
        "用过的让位给还没用过的"
    );
    // 落正文时那一段字**回给界面**（核心不碰编辑器手里那一份）
    let receipt = store.adopt_spark(untouched, "body", None, "", "test").unwrap();
    assert_eq!(receipt.text, "还没用过的");
}

#[test]
fn discarding_is_a_soft_delete_that_can_be_undone() {
    let (_dir, mut store) = fresh();
    let (work, _first, _second) = book(&mut store);
    let spark = jot(&mut store, work, "这条先不要了", vec![]);

    let left = store.discard_spark(spark, None, "test").unwrap();
    assert!(left.iter().all(|s| s.id != spark), "丢掉的马上不在这一屏上");
    assert!(store.fragment(spark).is_err(), "软删之后按「取一条」就查不到了");
    assert_eq!(used(&store, spark), 0, "丢弃**不是**用掉了：那本账一个字节不动");

    store.restore_fragment(spark, "test").unwrap();
    assert!(order(&store, work, None).contains(&spark), "捞回之后它又回到池子里");
    // 捞回来还是能采纳的（丢弃没把它弄坏）
    let receipt = store.adopt_spark(spark, "body", None, "", "test").unwrap();
    assert_eq!(receipt.text, "这条先不要了");
}

#[test]
fn a_question_card_cannot_be_adopted_as_a_spark() {
    let (_dir, mut store) = fresh();
    let (work, _first, _second) = book(&mut store);
    let card = store
        .create_question_card(&NewQuestionCard {
            work_id: work,
            body: "这一章要不要收线？".to_string(),
            source: "core".to_string(),
            template_key: String::new(),
            importance: 0.5,
            linked: vec![],
            derived_from: None,
            auto_derived: false,
        })
        .unwrap();

    let err = store.adopt_spark(card, "body", None, "", "test").unwrap_err();
    assert_eq!(err.code(), codes::SPARK_NOT_FOUND);
    let err = store.discard_spark(card, None, "test").unwrap_err();
    assert_eq!(err.code(), codes::SPARK_NOT_FOUND, "丢弃也不许碰问题卡");
}

#[test]
fn outline_and_scene_need_a_chapter_but_event_does_not() {
    let (_dir, mut store) = fresh();
    let (work, _first, _second) = book(&mut store);
    let spark = jot(&mut store, work, "还没想好放哪", vec![]);

    let err = store.adopt_spark(spark, "outline", None, "", "test").unwrap_err();
    assert_eq!(err.code(), codes::SPARK_NEEDS_CHAPTER);
    let err = store.adopt_spark(spark, "scene", None, "", "test").unwrap_err();
    assert_eq!(err.code(), codes::SPARK_NEEDS_CHAPTER);
    // 事件池不挂在章上：照落
    assert!(store.adopt_spark(spark, "event", None, "", "test").unwrap().event_id.is_some());
}
