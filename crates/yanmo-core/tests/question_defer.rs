//! 叩问「延后队列与重出条件」验收。
//!
//! 判据全部落在**可核对的事实**上：卡延后之后确实离开了候选池、条件**没到就不回来**、
//! 到了才回来并留下证据、作者说"我自己想起来再问"就真的不自动回来、
//! 锚点被删掉的那件事**翻篇**而不是永远堵着、延后降权压得下去但**不清零**。

use yanmo_core::model::{NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::question::{chapter_anchor, DeferCondition, DeferKind, DeferPreset, DAY_MS};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 建一本书：第一章有正文，第二章空着；返回（作品 id, 第二章 id）。
fn seeded(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let ch1 = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    let ch2 = store.create_node(work.id, None, NodeKind::Chapter, "第二章").unwrap();
    store.write_body(ch1, "第一章的正文。").unwrap();
    (work.id, ch2)
}

/// 一张挂在某章上的问题卡（锚点写进 `linked`，与生成器同一种写法）。
fn card(work_id: i64, chapter: Option<i64>) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: "这条问题先放一放".to_string(),
        source: "core".to_string(),
        template_key: "chapter.empty_body".to_string(),
        importance: 0.5,
        linked: chapter.map(|id| vec![format!("chapter:{id}")]).unwrap_or_default(),
        derived_from: None,
        auto_derived: false,
    }
}

fn state_of(store: &Store, id: i64) -> QuestionState {
    store.question_card(id).unwrap().state
}

#[test]
fn a_deferred_card_leaves_the_pool_and_comes_back_when_the_time_comes() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work, Some(chapter))).unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();

    let now = yanmo_core::time::now_millis();
    let deferral = store
        .defer_question_card(
            id,
            DeferCondition::after_ms(now + DAY_MS),
            " 等我先想清楚这一章的落点 ",
            "author",
        )
        .unwrap();

    assert_eq!(state_of(&store, id), QuestionState::Deferred, "延后要真的离开候选池");
    assert!(store.select_questions(work, 10).unwrap().is_empty(), "延后的卡不在候选池里");
    let open = store.open_deferrals(work).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].kind, DeferKind::Time);
    assert_eq!(open[0].note, "等我先想清楚这一章的落点", "作者填的那句要原样留着（首尾空白不算）");

    // 时间没到：一动不动（连记录也不标）
    assert!(store.requeue_due_questions(work, now + 3_600_000, "timer").unwrap().is_empty());
    assert_eq!(state_of(&store, id), QuestionState::Deferred);

    // 时间到了：回到候选池，并留下证据
    let requeued = store.requeue_due_questions(work, now + 2 * DAY_MS, "timer").unwrap();
    assert_eq!(requeued, vec![id]);
    assert_eq!(state_of(&store, id), QuestionState::Pending);
    assert!(store.select_questions(work, 10).unwrap().iter().any(|q| q.card_id == id));
    let history = store.card_deferrals(id).unwrap();
    assert_eq!(history[0].id, deferral);
    assert!(history[0].resolved_at.is_some(), "重出要标掉那条记录");
    assert!(store.open_deferrals(work).unwrap().is_empty());
    let event = store.card_events(id).unwrap().pop().unwrap();
    assert_eq!(event.op, "requeue");
    assert_eq!(event.from, "deferred");
    assert_eq!(event.trigger, "timer:time", "留痕里要看出是**哪条条件**到了");
}

#[test]
fn the_written_condition_fires_when_that_place_gets_written() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    // 锚点是一卷：写进这一卷里的任何一章都算「到了」
    let volume = store.create_node(work, None, NodeKind::Volume, "第二卷").unwrap();
    let inside = store.create_node(work, Some(volume), NodeKind::Chapter, "卷内第一章").unwrap();

    let by_chapter = store.create_question_card(&card(work, Some(chapter))).unwrap();
    let by_volume = store.create_question_card(&card(work, None)).unwrap();
    for (id, anchor) in [(by_chapter, chapter), (by_volume, volume)] {
        store.move_question_card(id, QuestionState::Asked, "push").unwrap();
        store
            .defer_question_card(id, DeferCondition::when_written(anchor), "", "author")
            .unwrap();
    }

    let now = yanmo_core::time::now_millis();
    assert!(
        store.requeue_due_questions(work, now, "timer").unwrap().is_empty(),
        "两处都还没写，谁也不该回来"
    );

    store.write_body(inside, "卷内第一章的正文。").unwrap();
    assert_eq!(
        store.requeue_due_questions(work, now, "timer").unwrap(),
        vec![by_volume],
        "写进这一卷了 → 卷锚点那条重出"
    );
    assert_eq!(state_of(&store, by_chapter), QuestionState::Deferred, "别的章没写，它还得等着");

    store.write_body(chapter, "第二章的正文。").unwrap();
    assert_eq!(store.requeue_due_questions(work, now, "timer").unwrap(), vec![by_chapter]);
}

#[test]
fn only_when_asked_never_comes_back_by_itself() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work, Some(chapter))).unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();
    store
        .defer_question_card(id, DeferCondition::manual(), "", "author")
        .unwrap();

    assert!(
        store.requeue_due_questions(work, i64::MAX / 2, "timer").unwrap().is_empty(),
        "「我自己想起来再问」就是不会自己回来"
    );
    assert_eq!(state_of(&store, id), QuestionState::Deferred);

    // 作者手动捞回之后，那条记录不该一直挂着
    store.move_question_card(id, QuestionState::Pending, "author").unwrap();
    assert!(store.requeue_due_questions(work, i64::MAX / 2, "timer").unwrap().is_empty());
    assert!(store.open_deferrals(work).unwrap().is_empty(), "旧记录被收掉");
    assert!(store.card_deferrals(id).unwrap()[0].resolved_at.is_some());
}

#[test]
fn a_deleted_anchor_counts_as_settled() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work, Some(chapter))).unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();
    store
        .defer_question_card(id, DeferCondition::when_written(chapter), "", "author")
        .unwrap();

    // 作者把那一章删了：这件事翻篇——让删掉的东西永远堵着一条问题，才是真的死路
    store.soft_delete_node(chapter).unwrap();
    assert_eq!(store.requeue_due_questions(work, 0, "timer").unwrap(), vec![id]);
    assert_eq!(state_of(&store, id), QuestionState::Pending);
}

#[test]
fn deferring_refuses_a_moment_that_already_passed_and_missing_anchors() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work, Some(chapter))).unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();

    let now = yanmo_core::time::now_millis();
    let err = store
        .defer_question_card(id, DeferCondition::after_ms(now - 1), "", "author")
        .unwrap_err();
    assert_eq!(err.code(), "question.defer_time_not_future");

    // 卡上没有章节锚点时，「写完这一章再问」这一档给不出条件（也就不该硬凑一个）
    let loose = store.create_question_card(&card(work, None)).unwrap();
    store.move_question_card(loose, QuestionState::Asked, "push").unwrap();
    let err = store
        .defer_question_card_by_preset(loose, DeferPreset::WhenChapterWritten, "", "author")
        .unwrap_err();
    assert_eq!(err.code(), "question.defer_needs_anchor");
    assert_eq!(state_of(&store, loose), QuestionState::Asked, "报错时状态不许动");

    // 有锚点的卡走预置档：到点落在"一天后"
    let ok = store
        .defer_question_card_by_preset(id, DeferPreset::AfterOneDay, "明天再说", "author")
        .unwrap();
    assert!(ok > 0);
    let due = store.open_deferrals(work).unwrap()[0].due_at_ms.unwrap();
    assert!(
        (due - (now + DAY_MS)).abs() < 5_000,
        "预置「一天后」应当落在一天之后：{due} vs {}",
        now + DAY_MS
    );

    // 卡没被问过就延后：状态机当场拒绝（延后只有「已问」这一条来路）
    let fresh_card = store.create_question_card(&card(work, Some(chapter))).unwrap();
    let err = store
        .defer_question_card(fresh_card, DeferCondition::manual(), "", "author")
        .unwrap_err();
    assert_eq!(err.code(), "card.illegal_transition");
}

#[test]
fn repeated_deferrals_push_the_card_back_without_killing_it() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let once = store.create_question_card(&card(work, Some(chapter))).unwrap();
    let many = store.create_question_card(&card(work, Some(chapter))).unwrap();

    // 把第二张卡"延后 → 重出"来回三轮
    let now = yanmo_core::time::now_millis();
    for round in 0..3 {
        store.move_question_card(many, QuestionState::Asked, "push").unwrap();
        store
            .defer_question_card(
                many,
                DeferCondition::after_ms(now + 60_000),
                "",
                "author",
            )
            .unwrap();
        let stamp = now + 120_000 + round * 1_000;
        assert_eq!(store.requeue_due_questions(work, stamp, "timer").unwrap(), vec![many]);
    }
    assert_eq!(store.card_deferrals(many).unwrap().len(), 3, "延后过几回，账上一目了然");

    let picked = store.select_questions(work, 10).unwrap();
    let of = |id: i64| picked.iter().find(|q| q.card_id == id).expect("该在候选里").gravity.clone();
    assert_eq!(of(once).defer_penalty, 1.0, "没延后过的不罚");
    assert_eq!(of(many).defer_penalty, 0.5, "延后到第三次起降权");
    assert!(of(many).total > 0.0, "降权不是清零——作者主动翻还看得见");
    assert_eq!(
        picked.iter().map(|q| q.card_id).collect::<Vec<_>>(),
        vec![once, many],
        "延后多的排后面（不硬插队），但还在池子里"
    );
}

#[test]
fn the_queue_is_readable_with_its_reasons() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work, Some(chapter))).unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();
    store
        .defer_question_card(id, DeferCondition::when_written(chapter), "等写到那儿", "author")
        .unwrap();

    let open = store.open_deferrals(work).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].card_id, id);
    assert_eq!(open[0].kind, DeferKind::Written);
    assert_eq!(open[0].anchor_node, Some(chapter));
    assert_eq!(open[0].note, "等写到那儿");
    assert!(open[0].resolved_at.is_none());

    // 卡的锚点解析：界面据此知道"写完这一章再问"说的是哪一章
    let stored = store.question_card(id).unwrap();
    assert_eq!(chapter_anchor(&stored.linked), Some(chapter));
}

#[test]
fn rerunning_v10_keeps_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let card_id;
    let work;
    {
        let mut store = Store::open(&path).unwrap();
        let (w, chapter) = seeded(&mut store);
        work = w;
        card_id = store.create_question_card(&card(w, Some(chapter))).unwrap();
        store.move_question_card(card_id, QuestionState::Asked, "push").unwrap();
        store
            .defer_question_card(
                card_id,
                DeferCondition::after_ms(yanmo_core::time::now_millis() + DAY_MS),
                "留着",
                "author",
            )
            .unwrap();
    }
    {
        let conn = yanmo_core::db::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 9).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let open = store.open_deferrals(work).unwrap();
    assert_eq!(open.len(), 1, "重跑 v10 不该弄丢延后队列");
    assert_eq!(open[0].note, "留着");
    assert_eq!(state_of(&store, card_id), QuestionState::Deferred);
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
}
