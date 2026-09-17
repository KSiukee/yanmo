//! 「计划 vs 实际」验收：**认得出谁在正文里、认得清哪条伏笔像被点到、只补不删且能退回**。
//!
//! 这一族的判据全是"字面可判"的：名字有没有出现、伏笔的片段有没有出现。
//! 所以这里盯的是三件事——**认人认准**（不把没写的报成写了）、**认人别被短名骗到**、
//! **写入口只加不减**（计划里列了、正文没认到的人，一个字都不许自动删）。

use yanmo_core::model::{
    EntityKind, ForeshadowState, NewEntityCard, NewForeshadow, NodeKind, SceneFields, WorkKind,
};
use yanmo_core::outline::ChapterState;
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn person(store: &mut Store, work_id: i64, name: &str, aliases: &[&str]) -> i64 {
    store
        .create_entity_card(
            &NewEntityCard {
                work_id,
                kind: EntityKind::Person,
                name: name.to_string(),
                aliases: aliases.iter().map(|a| a.to_string()).collect(),
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

fn summary(store: &mut Store, node_id: i64, text: &str) {
    store.set_node_summary(node_id, text).unwrap();
}

fn fields(store: &mut Store, node_id: i64, pov: &str) -> SceneFields {
    let value = SceneFields {
        node_id,
        pov: pov.to_string(),
        goal: String::new(),
        conflict: String::new(),
        outcome: String::new(),
    };
    store.save_scene_fields(&value, "test").unwrap()
}

fn find<'a>(
    report: &'a yanmo_core::store::OutlineActualReport,
    node_id: i64,
) -> Option<&'a yanmo_core::outline::ChapterActual> {
    report.chapters.iter().find(|chapter| chapter.node_id == node_id)
}

#[test]
fn a_written_chapter_whose_plan_matches_is_marked_written() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文", &["陆大人"]);
    summary(&mut store, chapters[0], "陆文进城");
    fields(&mut store, chapters[0], "陆文");
    store.set_node_cast(chapters[0], &[lu], "test").unwrap();
    store.write_body(chapters[0], "陆文推开城门，走了进去。").unwrap();

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[0]).expect("这一章该在对账清单里");
    assert_eq!(chapter.state, ChapterState::Written);
    assert_eq!(chapter.extra.len(), 0, "没有多出来的人");
    assert_eq!(chapter.missing.len(), 0, "计划里的人都认到了");
}

#[test]
fn a_planned_person_that_never_shows_up_is_missing_but_never_removed() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文", &[]);
    let zhang = person(&mut store, work, "老张", &[]);
    summary(&mut store, chapters[0], "两人碰头");
    store.set_node_cast(chapters[0], &[lu, zhang], "test").unwrap();
    store.write_body(chapters[0], "陆文一个人坐着。").unwrap();

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[0]).unwrap();
    assert_eq!(chapter.state, ChapterState::Deviated);
    assert_eq!(
        chapter.missing.iter().map(|m| m.name.clone()).collect::<Vec<_>>(),
        vec!["老张"],
        "计划了、正文没认到 → 只报，不动"
    );

    // 只补不删：把"正文里出现的人"补进去，老张还在
    let added = store.align_outline_cast(chapters[0], &[lu]).unwrap();
    assert_eq!(added, 0, "已经在计划里的人不该重复写");
    let cast = store.node_cast_of(chapters[0]).unwrap();
    assert_eq!(cast.len(), 2, "一个都不许自动删");
}

#[test]
fn a_person_in_the_prose_but_not_in_the_plan_shows_up_as_extra() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文", &[]);
    let zhang = person(&mut store, work, "老张", &[]);
    summary(&mut store, chapters[0], "陆文进城");
    store.set_node_cast(chapters[0], &[lu], "test").unwrap(); // 计划里只有陆文
    store.write_body(chapters[0], "陆文回头，看见老张站在檐下。").unwrap();

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[0]).unwrap();
    assert_eq!(
        chapter.extra.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
        vec!["老张"],
        "正文里出现了、计划里没有 → 可以一键补"
    );
    assert_eq!(
        chapter.matched.iter().map(|m| m.name.clone()).collect::<Vec<_>>(),
        vec!["陆文"],
        "计划里的人也真的在正文里"
    );
    assert!(chapter.missing.is_empty());

    let added = store.align_outline_cast(chapters[0], &[zhang]).unwrap();
    assert_eq!(added, 1);
    assert_eq!(store.node_cast_of(chapters[0]).unwrap().len(), 2, "补进来一个，原来的还在");

    // 补完再对一遍：不再报"多出来"
    let again = store.outline_actuals(work).unwrap();
    let chapter = find(&again, chapters[0]).unwrap();
    assert!(chapter.extra.is_empty(), "补过之后就该对上了：{:?}", chapter.extra);
}

#[test]
fn a_planned_chapter_without_prose_is_unwritten_and_an_empty_one_is_not_reported() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    summary(&mut store, chapters[0], "打算这么写");

    let report = store.outline_actuals(work).unwrap();
    assert_eq!(find(&report, chapters[0]).unwrap().state, ChapterState::Unwritten);
    assert!(find(&report, chapters[1]).is_none(), "没计划也没正文：还没轮到它，不报");
    assert!(find(&report, chapters[2]).is_none());
}

#[test]
fn prose_without_any_plan_counts_as_deviated() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    store.write_body(chapters[0], "随手写了一段，什么计划都没填。").unwrap();

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[0]).unwrap();
    assert_eq!(chapter.state, ChapterState::Deviated);
    assert!(chapter.planned.is_empty(), "计划就是一字没填");
}

/// 短名字被长名字盖住时**不算出现**：正文里的"小明"不该把"明"那张卡也报成写过。
#[test]
fn a_short_name_covered_by_a_longer_one_is_not_counted() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let ming = person(&mut store, work, "明", &[]);
    let xiao_ming = person(&mut store, work, "小明", &[]);
    summary(&mut store, chapters[0], "只有一个名字出现");
    store.write_body(chapters[0], "小明抬起头。").unwrap();
    let _ = (ming, xiao_ming);

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[0]).unwrap();
    let names: Vec<String> = chapter.extra.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, vec!["小明"], "长的先占位，短的被盖住就不算：{names:?}");
}

/// 别称也算出现（作者给"陆文"记了"陆大人"，正文里只写了别称）。
#[test]
fn an_alias_counts_as_an_appearance() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文", &["陆大人"]);
    summary(&mut store, chapters[0], "陆文进城");
    store.set_node_cast(chapters[0], &[lu], "test").unwrap();
    store.write_body(chapters[0], "“陆大人到——”门外喊了一声。").unwrap();

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[0]).unwrap();
    assert_eq!(chapter.state, ChapterState::Written, "别称认到了就算写了");
    assert!(chapter.missing.is_empty());
}

/// 伏笔：还埋着的、正文里有相近说法 → 报候选；收掉的就不再念。
#[test]
fn planted_foreshadows_are_reported_as_candidates_and_collected_ones_are_not() {
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
    store.write_body(chapters[1], "他摸了摸玉佩上的裂纹，又想起老张的怀表。").unwrap();
    // 第二条收掉：不该再报
    store
        .move_foreshadow(collected, ForeshadowState::Collected, Some(chapters[2]), "test")
        .unwrap();

    let report = store.outline_actuals(work).unwrap();
    let chapter = find(&report, chapters[1]).unwrap();
    let ids: Vec<i64> = chapter.foreshadows.iter().map(|hit| hit.foreshadow_id).collect();
    assert_eq!(ids, vec![planted], "只报还埋着的那一条：{ids:?}");
    assert!(!chapter.foreshadows[0].fragments.is_empty(), "要能说清凭什么说像");
    // 收掉的那一条状态也确实是收了的（免得这条测试自己骗自己）
    assert_eq!(store.foreshadow(collected).unwrap().state, ForeshadowState::Collected);
}

#[test]
fn a_missing_foreshadow_wording_is_not_reported() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    store
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
    store.write_body(chapters[1], "这一段跟那条线头毫无关系。").unwrap();

    let report = store.outline_actuals(work).unwrap();
    assert!(find(&report, chapters[1]).unwrap().foreshadows.is_empty(), "字面不像就不报");
}

/// 对齐之前先留底；撤销能把这一章的大纲放回去（含"把补进来的人撤掉"）。
#[test]
fn aligning_leaves_a_snapshot_and_undo_puts_the_outline_back() {
    let (_dir, mut store) = fresh();
    let (work, chapters) = book(&mut store);
    let lu = person(&mut store, work, "陆文", &[]);
    let zhang = person(&mut store, work, "老张", &[]);
    summary(&mut store, chapters[0], "原本的章纲");
    store.write_body(chapters[0], "陆文回头，看见老张。").unwrap();

    assert!(store.latest_outline_snapshot(chapters[0]).unwrap().is_none(), "还没对齐过：没有留底");
    let added = store.align_outline_cast(chapters[0], &[zhang]).unwrap();
    assert_eq!(added, 1);

    let snapshot = store.latest_outline_snapshot(chapters[0]).unwrap().expect("对齐前该留一份底");
    assert_eq!(snapshot.note, "align_cast");
    assert_eq!(store.node_cast_of(chapters[0]).unwrap().len(), 1);
    assert_eq!(store.node_cast_of(chapters[0]).unwrap()[0].entity_id, zhang);

    // 撤销：出场人物回到改之前（空），章纲一字不动
    store.restore_outline_snapshot(snapshot.id).unwrap();
    assert!(store.node_cast_of(chapters[0]).unwrap().is_empty(), "撤销要把补进来的人撤掉");
    assert_eq!(store.read_body(chapters[0]).unwrap(), "陆文回头，看见老张。", "正文一个字不动");

    // 补一个"已经在计划里"的人：不该留底、不该写库
    store.set_node_cast(chapters[0], &[lu], "test").unwrap();
    let before = store.latest_outline_snapshot(chapters[0]).unwrap().map(|s| s.id);
    assert_eq!(store.align_outline_cast(chapters[0], &[lu]).unwrap(), 0);
    assert_eq!(
        store.latest_outline_snapshot(chapters[0]).unwrap().map(|s| s.id),
        before,
        "没变化就不该再留一份底"
    );
}
