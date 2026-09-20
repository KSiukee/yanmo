//! 机械检查项的**读数**验收：数得对、读得齐，两种视角各一条。
//!
//! 判据本身（认人最长优先 / 伏笔片段）在 `outline_actual.rs` 那一族里已经验过——
//! 这里只验 `store::sop_mech` 把同样那套判据读齐之后，数给界面看的东西对不对。
//!
//! 一句话口径：**核心零文案**——这里断言的是事实（缺了谁、几章有问题），不是句子。

use yanmo_core::model::{
    EntityKind, ForeshadowState, NewEntityCard, NewForeshadow, NodeKind, WorkKind,
};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn person(store: &mut Store, work_id: i64, name: &str) -> i64 {
    store
        .create_entity_card(
            &NewEntityCard {
                work_id,
                kind: EntityKind::Person,
                name: name.to_string(),
                aliases: Vec::new(),
                attributes: Vec::new(),
                note: String::new(),
            },
            "test",
        )
        .unwrap()
}

/// 一本书 + 三章（都在同一个根卷下）。
fn book(store: &mut Store) -> (i64, Vec<i64>) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for title in ["第一章", "第二章", "第三章"] {
        chapters.push(store.create_node(work.id, Some(volume), NodeKind::Chapter, title).unwrap());
    }
    (work.id, chapters)
}

#[test]
fn chapter_facts_count_who_is_missing_and_who_is_extra() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文");
    let shen = person(&mut store, work, "沈砚");
    let stranger = person(&mut store, work, "卖炭翁");
    store.set_node_cast(chapters[0], &[lu, shen], "test").unwrap();
    // 计划挂了两个人，正文里只到了一个、还多出一个没挂的
    store.write_body(chapters[0], "陆文推开城门，卖炭翁在墙根打盹。").unwrap();

    let facts = store.chapter_mech_facts(chapters[0]).unwrap();
    assert_eq!(facts.node_id, chapters[0]);
    assert!(facts.has_body);
    assert_eq!(facts.planned, 2);
    assert_eq!(facts.matched, 1);
    assert_eq!(names(&facts.missing), vec!["沈砚"], "计划里挂了、正文没认到");
    assert_eq!(names(&facts.extra), vec!["卖炭翁"], "正文里出现、计划没挂");
    assert!(facts.extra.iter().any(|found| found.entity_id == stranger));
    assert!(facts.foreshadows.is_empty(), "一条伏笔都没埋：没有候选");
    assert_eq!(facts.planted_open, 0);
}

#[test]
fn a_chapter_without_prose_still_tells_the_truth_but_says_so() {
    // 事实如实（一个都认不到），界面靠 `has_body` 说"还没动笔"——**不是**"写漏了"
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文");
    store.set_node_cast(chapters[0], &[lu], "test").unwrap();

    let facts = store.chapter_mech_facts(chapters[0]).unwrap();
    assert!(!facts.has_body);
    assert_eq!(facts.matched, 0);
    assert_eq!(names(&facts.missing), vec!["陆文"]);
    assert!(facts.extra.is_empty());
}

#[test]
fn book_facts_count_the_chapters_that_deviate() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文");
    let shen = person(&mut store, work, "沈砚");
    // 第一章：计划两个人，正文只到陆文 → 有 missing
    store.set_node_cast(chapters[0], &[lu, shen], "test").unwrap();
    store.write_body(chapters[0], "陆文推开城门。").unwrap();
    // 第二章：计划只有陆文，正文多出沈砚 → 有 extra（missing 为空）
    store.set_node_cast(chapters[1], &[lu], "test").unwrap();
    store.write_body(chapters[1], "陆文与沈砚对坐。").unwrap();
    // 第三章：没计划也没正文 → 这一章不该进对账清单
    let facts = store.book_mech_facts(work).unwrap();

    assert_eq!(facts.chapters, 2, "没计划也没正文的章不算在内");
    assert_eq!(facts.cast_missing, 1);
    assert_eq!(facts.cast_extra, 1);
    assert_eq!(facts.foreshadow, 0);
    assert_eq!(facts.truncated, 0);
}

#[test]
fn foreshadow_facts_are_candidates_and_collected_ones_stop_counting() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let planted = store
        .create_foreshadow(
            &NewForeshadow {
                work_id: work,
                body: "玉佩上的裂纹".to_string(),
                planted_node: Some(chapters[0]),
                note: String::new(),
            },
            "test",
        )
        .unwrap();
    let collected = store
        .create_foreshadow(
            &NewForeshadow {
                work_id: work,
                body: "老张的怀表".to_string(),
                planted_node: Some(chapters[0]),
                note: String::new(),
            },
            "test",
        )
        .unwrap();
    store
        .move_foreshadow(collected, ForeshadowState::Collected, Some(chapters[2]), "test")
        .unwrap();
    store.write_body(chapters[1], "他摸了摸玉佩上的裂纹，又想起老张的怀表。").unwrap();

    let facts = store.chapter_mech_facts(chapters[1]).unwrap();
    assert_eq!(facts.planted_open, 1, "收掉的那条不再念");
    let ids: Vec<i64> = facts.foreshadows.iter().map(|hit| hit.foreshadow_id).collect();
    assert_eq!(ids, vec![planted], "只报还埋着的那一条：{ids:?}");
    assert!(!facts.foreshadows[0].fragments.is_empty(), "要能说清凭什么说像");

    let all = store.book_mech_facts(work).unwrap();
    assert_eq!(all.foreshadow, 1, "有伏笔候选的是第二章这一章");
    assert_eq!(all.planted_open, 1);
}

/// 只取名字，断言好读（顺序按卡名排，与核心一致）。
fn names(refs: &[yanmo_core::outline::CastRef]) -> Vec<&str> {
    refs.iter().map(|item| item.name.as_str()).collect()
}
