//! 主动问一句（推）的验收：**门槛管住、开口算数、只推不写**。
//!
//! 判据落在可核对的事实上：配额与冷却真的挡得住、**空章里那两句不成立所以不问**、
//! 推过的卡真的走到「已问」且留下 `push:<时机>` 的痕、账与卡同一个事务、
//! 池子空了就明确说"没得问"而不是硬造一条。

use yanmo_core::model::{NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::question::{PushQuota, PushReason};
use yanmo_core::store::{PushOutcome, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn card(work_id: i64, body: &str) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: body.to_string(),
        source: "core".to_string(),
        template_key: "chapter.empty_body".to_string(),
        importance: 0.5,
        linked: vec!["chapter:1".to_string()],
        derived_from: None,
        auto_derived: false,
    }
}

/// 一本书 + 一章**写了正文**的章（配额与冷却那几条谈的是门槛，章别空着）。
fn seeded(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    store.write_body(chapter, "第一章的正文。").unwrap();
    (work.id, chapter)
}

/// 一本书 + 一章**还空着**的章（空章判据那条用）。
fn seeded_empty(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    (work.id, chapter)
}

const DAY: i64 = 2026_09_16;
const QUOTA: PushQuota = PushQuota { per_day: 3, cooldown_minutes: 60 };

#[test]
fn pushing_asks_the_top_question_once_and_records_the_account() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let first = store.create_question_card(&card(work, "第一条")).unwrap();
    store.create_question_card(&card(work, "第二条")).unwrap();

    let outcome = store
        .push_question(work, Some(chapter), QUOTA, DAY, PushReason::NewChapter)
        .unwrap();
    let PushOutcome::Pushed { question } = &outcome else {
        panic!("这种时候应该问得出来：{outcome:?}");
    };
    assert_eq!(question.card_id, first, "推的是候选里最靠前的那张");

    // 算问过：状态走到「已问」，痕里写清是哪条时机推的
    assert_eq!(store.question_card(first).unwrap().state, QuestionState::Asked);
    let events = store.card_events(first).unwrap();
    let last = events.last().unwrap();
    assert_eq!((last.op.as_str(), last.to.as_str()), ("ask", "asked"));
    assert_eq!(last.trigger, "push:new_chapter");
    assert_eq!(store.pushed_today(DAY).unwrap(), 1);

    // 冷却之内不再开口
    let again = store
        .push_question(work, Some(chapter), QUOTA, DAY, PushReason::Idle)
        .unwrap();
    assert_eq!(again, PushOutcome::TooSoon);
    assert_eq!(store.pushed_today(DAY).unwrap(), 1, "被挡下的那次不记账");
}

#[test]
fn the_quota_and_the_off_switch_both_hold_it_back() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    for index in 1..=4 {
        store.create_question_card(&card(work, &format!("第{index}条"))).unwrap();
    }

    // 0 次 = 不打扰：一次都不问
    let quiet = PushQuota { per_day: 0, cooldown_minutes: 0 };
    assert_eq!(
        store.push_question(work, Some(chapter), quiet, DAY, PushReason::Idle).unwrap(),
        PushOutcome::QuotaUsed
    );
    assert_eq!(store.pushed_today(DAY).unwrap(), 0);

    // 配额 2、无冷却：问两次，第三次挡住
    let two = PushQuota { per_day: 2, cooldown_minutes: 0 };
    for _ in 0..2 {
        assert!(matches!(
            store.push_question(work, Some(chapter), two, DAY, PushReason::Idle).unwrap(),
            PushOutcome::Pushed { .. }
        ));
    }
    assert_eq!(
        store.push_question(work, Some(chapter), two, DAY, PushReason::Idle).unwrap(),
        PushOutcome::QuotaUsed
    );
    assert_eq!(store.pushed_today(DAY).unwrap(), 2);

    // 第二天：配额重新开始（昨天的账不算今天的）
    let tomorrow = DAY + 1;
    assert_eq!(store.pushed_today(tomorrow).unwrap(), 0);
    assert!(matches!(
        store
            .push_question(work, Some(chapter), two, tomorrow, PushReason::NewChapter)
            .unwrap(),
        PushOutcome::Pushed { .. }
    ));
}

/// **空章不问**：空章里"你卡住了"与"这一章写完了"根本不成立——不推、不记账、
/// 连"算已问"都不该发生；而"这一章从哪儿开始"本来就是为这一章空着才问的，照旧。
#[test]
fn an_empty_chapter_is_not_stuck_and_not_done() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded_empty(&mut store);
    let card_id = store.create_question_card(&card(work, "第一条")).unwrap();

    for reason in [PushReason::Idle, PushReason::ChapterDone] {
        assert_eq!(
            store.push_question(work, Some(chapter), QUOTA, DAY, reason).unwrap(),
            PushOutcome::EmptyChapter,
            "{reason:?} 在空章里不成立"
        );
        assert_eq!(store.pushed_today(DAY).unwrap(), 0, "没问就不记账");
        assert_eq!(
            store.question_card(card_id).unwrap().state,
            QuestionState::Pending,
            "没问出去，卡不该被算成「已问」"
        );
    }

    // 开新章：照旧问得出来（它本来就是为这一章空着才问的）
    assert!(matches!(
        store
            .push_question(work, Some(chapter), QUOTA, DAY, PushReason::NewChapter)
            .unwrap(),
        PushOutcome::Pushed { .. }
    ));
    assert_eq!(store.question_card(card_id).unwrap().state, QuestionState::Asked);
    assert_eq!(store.pushed_today(DAY).unwrap(), 1);
}

/// 没打开任何一章（`node_id` 是 `None`）也按"没有正文"算：那两句话同样不成立。
#[test]
fn with_no_chapter_open_only_the_new_chapter_reason_may_ask() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    store.create_question_card(&card(work, "第一条")).unwrap();

    assert_eq!(
        store.push_question(work, None, QUOTA, DAY, PushReason::Idle).unwrap(),
        PushOutcome::EmptyChapter
    );
    assert!(matches!(
        store.push_question(work, None, QUOTA, DAY, PushReason::NewChapter).unwrap(),
        PushOutcome::Pushed { .. }
    ));
}

/// 非空的章：三条时机照旧（空章判据不该顺手把正常的推也拦掉）。
#[test]
fn a_written_chapter_keeps_all_three_reasons_working() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    // 无冷却：这一个用例谈的是"三条时机都成立"，不是冷却
    let quota = PushQuota { per_day: 3, cooldown_minutes: 0 };
    for reason in [PushReason::NewChapter, PushReason::Idle, PushReason::ChapterDone] {
        store.create_question_card(&card(work, "一条待问的")).unwrap();
        assert!(
            matches!(
                store.push_question(work, Some(chapter), quota, DAY, reason).unwrap(),
                PushOutcome::Pushed { .. }
            ),
            "{reason:?} 在写了正文的章里该照问"
        );
    }
    // 三条痕各记各的时机
    let asked = store.question_cards(work, Some(QuestionState::Asked)).unwrap();
    assert_eq!(asked.len(), 3);
    for (card, want) in asked.iter().zip(["new_chapter", "idle", "chapter_done"]) {
        let events = store.card_events(card.id).unwrap();
        assert_eq!(events.last().unwrap().trigger, format!("push:{want}"));
    }
}

#[test]
fn an_empty_pool_says_so_instead_of_making_something_up() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    assert_eq!(
        store.push_question(work, Some(chapter), QUOTA, DAY, PushReason::Idle).unwrap(),
        PushOutcome::NothingToAsk
    );
    assert_eq!(store.pushed_today(DAY).unwrap(), 0, "没得问就不记账");
}

#[test]
fn the_pushed_question_leaves_the_pool_but_the_panel_still_works() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.create_question_card(&card(work, "第一条")).unwrap();
    store.create_question_card(&card(work, "第二条")).unwrap();

    store
        .push_question(work, Some(chapter), QUOTA, DAY, PushReason::ChapterDone)
        .unwrap();
    // 推走了一条：候选池少一条，但**面板照常能列出剩下的**（配额只管推，不管拉）
    let rest = store.select_questions_for_chapter(work, chapter, 10).unwrap();
    assert_eq!(rest.len(), 1, "推过的那张已经不在池子里");
    assert!(store.question_cards(work, Some(QuestionState::Asked)).unwrap().len() == 1);
}
