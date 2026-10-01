//! 五线推进度验收：**段 ↔ 故事线**这条标注的读、写、与"没标过是什么样"。
//!
//! 六件事：
//! 1. 标了能读回来，且返回的是**稳定序**（与输入顺序无关）——一次拿全与单段读同一个口径；
//! 2. **整份覆盖**（第二次设短了，旧的线真没了）与**清空**（空数组）；
//! 3. 两章各自独立；没标过的章、不存在的段，都读成空（**没标 ≠ 没推进**）；
//! 4. 同一章同一条线重复传只留一份（表上有主键，入口还要先去重）；
//! 5. 认不出的码报错——写入口靠类型保证；库里被写坏时**读**也当场报，不静默跳过；
//! 6. 迁移 v19 幂等（同一个库开两次），且标注落库会留痕进 op-log。

use yanmo_core::error::codes;
use yanmo_core::model::{EntityKind, NewEntityCard, NodeKind, StoryLine, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书 + 一卷下的两章（返回 work id 与两章 id）。
fn book_with_two_chapters(store: &mut Store) -> (i64, i64, i64) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    (work.id, first, second)
}

fn op_log_count(store: &Store) -> i64 {
    store
        .conn()
        .query_row("SELECT COUNT(*) FROM op_log WHERE entity = 'node_lines'", [], |r| r.get(0))
        .unwrap()
}

/// 标了能读回来：单段读、整本读、以及**返回的稳定序与输入顺序无关**。
#[test]
fn lines_round_trip_and_come_back_in_a_stable_order() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book_with_two_chapters(&mut store);
    assert!(store.lines_of(first).unwrap().is_empty(), "没标过的章：空数组，不是报错");

    // 故意把输入打乱（还带个重复）：读回来必须是 ALL 的顺序（main 在 romance 前面）
    let saved = store
        .set_node_lines(first, &[StoryLine::Romance, StoryLine::Main, StoryLine::Romance], "test")
        .unwrap();
    assert_eq!(
        saved,
        vec![StoryLine::Main, StoryLine::Romance],
        "返回的是稳定序，且重复的那一条只留一份"
    );
    assert_eq!(store.lines_of(first).unwrap(), vec![StoryLine::Main, StoryLine::Romance]);

    // 一次拿全：只出现标过的章，每段的线也按 ALL 排
    let all = store.node_lines(work).unwrap();
    assert_eq!(all.len(), 1, "没标过的章不出现在结果里");
    assert_eq!(all[&first], vec![StoryLine::Main, StoryLine::Romance]);
    assert!(all.get(&second).is_none());
}

/// 整份覆盖 + 清空：第二次设短了，旧的线真没了；空数组＝这一章清空。
#[test]
fn setting_again_replaces_the_whole_annotation_and_an_empty_list_clears_it() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book_with_two_chapters(&mut store);
    store
        .set_node_lines(first, &[StoryLine::Main, StoryLine::Romance, StoryLine::Growth], "test")
        .unwrap();

    let after = store.set_node_lines(first, &[StoryLine::Suspense], "test").unwrap();
    assert_eq!(after, vec![StoryLine::Suspense], "整份覆盖：旧的线一条都不该留下");
    assert_eq!(store.lines_of(first).unwrap(), vec![StoryLine::Suspense]);

    let cleared = store.set_node_lines(first, &[], "test").unwrap();
    assert!(cleared.is_empty(), "空数组＝这一章清空");
    assert!(store.lines_of(first).unwrap().is_empty());
    assert!(
        !store.node_lines(work).unwrap().contains_key(&first),
        "清空之后这本书里不再有它（与从没标过同一个状态）"
    );
}

/// 两章各自独立；同一章重复给同一条线只留一份。
#[test]
fn chapters_are_independent_and_duplicates_collapse() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book_with_two_chapters(&mut store);
    store.set_node_lines(first, &[StoryLine::Main], "test").unwrap();
    store
        .set_node_lines(second, &[StoryLine::Growth, StoryLine::Growth, StoryLine::Main], "test")
        .unwrap();

    assert_eq!(store.lines_of(first).unwrap(), vec![StoryLine::Main], "动第二段不影响第一段");
    assert_eq!(
        store.lines_of(second).unwrap(),
        vec![StoryLine::Main, StoryLine::Growth],
        "重复给同一条只留一份（按 ALL 排）"
    );
    let all = store.node_lines(work).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[&first].len(), 1);
    assert_eq!(all[&second].len(), 2);
}

/// 不存在的段（含软删）：写被拒（`node.gone`），读是空——作者还没标不是故障。
#[test]
fn a_missing_node_is_refused_on_write_and_reads_empty() {
    let (_dir, mut store) = fresh();
    let (_work, first, _second) = book_with_two_chapters(&mut store);
    assert!(store.lines_of(9_999).unwrap().is_empty());

    let err = store.set_node_lines(9_999, &[StoryLine::Main], "test").unwrap_err();
    assert_eq!(err.code(), codes::NODE_GONE, "{err:?}");

    store.soft_delete_node(first).unwrap();
    assert!(
        store.set_node_lines(first, &[StoryLine::Main], "test").is_err(),
        "软删的段不能再往上标线"
    );
}

/// 认不出的码：`parse` 当场报（写入口靠类型保证），库里被写坏时读也当场报——不静默跳过。
#[test]
fn an_unknown_code_is_reported_instead_of_being_dropped() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book_with_two_chapters(&mut store);
    assert_eq!(StoryLine::parse("plot").unwrap_err().code(), codes::UNKNOWN_STORY_LINE);

    // 模拟"库里被别的东西写坏了"（绕过入口直接落一行）
    store
        .conn()
        .execute(
            &format!("INSERT INTO node_lines(node_id, line, created_at) VALUES({first}, 'plot', 0)"),
            [],
        )
        .unwrap();
    assert_eq!(store.lines_of(first).unwrap_err().code(), codes::UNKNOWN_STORY_LINE);
    assert_eq!(store.node_lines(work).unwrap_err().code(), codes::UNKNOWN_STORY_LINE);
}

/// v19 幂等：同一个库开两次，标注一条不少、op-log 不重复记；写出去的那次留了痕。
#[test]
fn migration_nineteen_is_idempotent_on_an_existing_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let (work, first) = {
        let mut store = Store::open(&path).unwrap();
        let (work, first, _second) = book_with_two_chapters(&mut store);
        store.set_node_lines(first, &[StoryLine::Main, StoryLine::Romance], "test").unwrap();
        assert_eq!(op_log_count(&store), 1, "作者改标注是有动作的写：必须留痕");
        // 退回上一版再开一次：v19 得能重跑（`CREATE ... IF NOT EXISTS` 兜着）
        store.conn().pragma_update(None, "user_version", 18).unwrap();
        (work, first)
    };

    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
    assert_eq!(store.lines_of(first).unwrap(), vec![StoryLine::Main, StoryLine::Romance]);
    assert_eq!(store.node_lines(work).unwrap()[&first].len(), 2);
    assert_eq!(op_log_count(&store), 1, "重开库不该再记一次 op-log");
}

/// 闸（用户拍板）：**只有「正文单位」能标线**——卷与场景卡当场拒，且拒的话里带上种类。
///
/// 判据与数章是同一份（`NodeKind::is_text_spine`）：所以这里挨个过一遍五类，
/// 谁被拒、谁被放行都得与"表里数得出几章"对得上。
#[test]
fn only_text_units_can_be_annotated() {
    let (_dir, mut store) = fresh();
    let (work, chapter, _second) = book_with_two_chapters(&mut store);
    // 小说的根就是那个卷（`create_work` 会给一本小说配一个）
    let volume = store.list_nodes(work).unwrap()[0].id;
    let scene = store.create_node(work, Some(chapter), NodeKind::Scene, "开场").unwrap();
    let section = store.create_node(work, Some(chapter), NodeKind::Section, "第一节").unwrap();

    for (node, kind_code) in [(volume, "volume"), (scene, "scene")] {
        let err = store.set_node_lines(node, &[StoryLine::Main], "test").unwrap_err();
        assert_eq!(err.code(), codes::STORY_LINE_NODE_NO_BODY, "{err:?}");
        let params: std::collections::BTreeMap<_, _> = err.params().into_iter().collect();
        assert_eq!(
            params.get("kind").map(String::as_str),
            Some(kind_code),
            "拒的时候要说清它是什么种类（界面据此说\"这是卷 / 这是场景卡\"）：{err:?}"
        );
        assert!(store.lines_of(node).unwrap().is_empty(), "拒了就不许留下一个字");
    }

    for node in [chapter, section] {
        assert_eq!(
            store.set_node_lines(node, &[StoryLine::Main], "test").unwrap(),
            vec![StoryLine::Main],
            "章与节都能标"
        );
    }

    // 单篇（零层级作品的那一篇正文，本身就是一段正文）
    let article = store.create_work(WorkKind::Article, "零层级").unwrap();
    let piece = store.list_nodes(article.id).unwrap()[0].id;
    assert_eq!(store.list_nodes(article.id).unwrap()[0].kind, NodeKind::Piece);
    assert_eq!(
        store.set_node_lines(piece, &[StoryLine::Growth], "test").unwrap(),
        vec![StoryLine::Growth],
        "单篇自己就是一段正文，能标"
    );
}

/// 拒要**干净**：被拒的那一次，库里本来标着的那几条一个字都不动，也不留痕。
#[test]
fn a_refused_write_leaves_the_existing_annotation_untouched() {
    let (_dir, mut store) = fresh();
    let (work, chapter, _second) = book_with_two_chapters(&mut store);
    let volume = store.list_nodes(work).unwrap()[0].id;
    store
        .set_node_lines(chapter, &[StoryLine::Main, StoryLine::Romance], "test")
        .unwrap();
    let before = store.node_lines(work).unwrap();
    let op_before = op_log_count(&store);

    let err = store.set_node_lines(volume, &[], "test").unwrap_err();
    assert_eq!(err.code(), codes::STORY_LINE_NODE_NO_BODY, "{err:?}");

    assert_eq!(
        store.lines_of(chapter).unwrap(),
        vec![StoryLine::Main, StoryLine::Romance],
        "被拒的是卷，别的段的标注一个字都不许动"
    );
    assert_eq!(store.node_lines(work).unwrap(), before, "整本书读回来还是原来那一份");
    assert_eq!(op_log_count(&store), op_before, "被拒的那次不该留痕");
}

/// 一张人物卡（对账那一步要用的人）。
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

/// 「不进」：大纲留底 / 撤回**不碰**五线标注——界面上那句
/// "大纲撤回不会回滚这里标过的线"说的就是这件事（撤回那一支只写回章纲 / 四格 / 出场人物）。
#[test]
fn restoring_an_outline_snapshot_does_not_roll_back_the_story_lines() {
    let (_dir, mut store) = fresh();
    let (work, chapter, _second) = book_with_two_chapters(&mut store);
    let card = person(&mut store, work, "陆文");
    store.set_node_summary(chapter, "他进城").unwrap();
    store.set_node_lines(chapter, &[StoryLine::Main], "test").unwrap();

    // 对一次账（补一个人）：它改之前会先给这一章留一份底 —— 撤回认的就是那一份
    assert_eq!(store.align_outline_cast(chapter, &[card]).unwrap(), 1);
    let snapshot = store.latest_outline_snapshot(chapter).unwrap().unwrap();
    store.set_node_summary(chapter, "改成别的").unwrap();
    store.set_node_lines(chapter, &[StoryLine::Suspense], "test").unwrap();

    store.restore_outline_snapshot(snapshot.id).unwrap();
    assert_eq!(store.node_summary(chapter).unwrap(), "他进城", "留底里那句话回来了");
    assert_eq!(
        store.lines_of(chapter).unwrap(),
        vec![StoryLine::Suspense],
        "五线留着**后来**改的那一份：大纲撤回不回滚它"
    );
}
