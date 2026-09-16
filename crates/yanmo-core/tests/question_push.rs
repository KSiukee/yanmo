//! 主动问一句（推）的验收：**门槛管住、开口算数、只推不写**。
//!
//! 判据落在可核对的事实上：配额与冷却真的挡得住、推过的卡真的走到「已问」且留下
//! `push:<时机>` 的痕、账与卡同一个事务、池子空了就明确说"没得问"而不是硬造一条。

use yanmo_core::model::{NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::question::PushQuota;
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

fn seeded(store: &mut Store) -> (i64, i64) {
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

    let outcome = store.push_question(work, Some(chapter), QUOTA, DAY, "new_chapter").unwrap();
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
    let again = store.push_question(work, Some(chapter), QUOTA, DAY, "idle").unwrap();
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
        store.push_question(work, Some(chapter), quiet, DAY, "idle").unwrap(),
        PushOutcome::QuotaUsed
    );
    assert_eq!(store.pushed_today(DAY).unwrap(), 0);

    // 配额 2、无冷却：问两次，第三次挡住
    let two = PushQuota { per_day: 2, cooldown_minutes: 0 };
    for _ in 0..2 {
        assert!(matches!(
            store.push_question(work, Some(chapter), two, DAY, "idle").unwrap(),
            PushOutcome::Pushed { .. }
        ));
    }
    assert_eq!(
        store.push_question(work, Some(chapter), two, DAY, "idle").unwrap(),
        PushOutcome::QuotaUsed
    );
    assert_eq!(store.pushed_today(DAY).unwrap(), 2);

    // 第二天：配额重新开始（昨天的账不算今天的）
    let tomorrow = DAY + 1;
    assert_eq!(store.pushed_today(tomorrow).unwrap(), 0);
    assert!(matches!(
        store.push_question(work, Some(chapter), two, tomorrow, "new_chapter").unwrap(),
        PushOutcome::Pushed { .. }
    ));
}

#[test]
fn an_empty_pool_says_so_instead_of_making_something_up() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    assert_eq!(
        store.push_question(work, Some(chapter), QUOTA, DAY, "idle").unwrap(),
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

    store.push_question(work, Some(chapter), QUOTA, DAY, "chapter_done").unwrap();
    // 推走了一条：候选池少一条，但**面板照常能列出剩下的**（配额只管推，不管拉）
    let rest = store.select_questions_for_chapter(work, chapter, 10).unwrap();
    assert_eq!(rest.len(), 1, "推过的那张已经不在池子里");
    assert!(store.question_cards(work, Some(QuestionState::Asked)).unwrap().len() == 1);
}
