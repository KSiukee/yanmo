//! 模块叩问通道验收：**一个出口、来源可辨、限频、未声明/未激活提不进来、
//! 有据才问、核心不被挤掉**。
//!
//! 这一组只盯"从模块进来"的那条路；叩问本身的状态机、选题与静音各有自己的验收文件。
//! 模块地基（能力清单 / 生命周期）还没落地，所以凭据由测试直接构造——规则本身
//! 不依赖模块系统怎么实现，只依赖宿主**如实回答**"声明过没有 / 现在激活吗"。

use std::time::Duration;

use yanmo_core::error_codes::codes;
use yanmo_core::model::{
    ModuleQuestion, ModuleQuestionGrant, NewQuestionCard, NodeKind, QuestionDropReason,
    QuestionState, WorkKind,
};
use yanmo_core::question::{PushQuota, PushReason};
use yanmo_core::store::{ModuleIntakePolicy, PushOutcome, QuestionIntakeOutcome, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书 + 一章写了正文的章（推的门槛要"这一章不是空的"）。
fn seeded(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    store.write_body(chapter, "第一章的正文。").unwrap();
    (work.id, chapter)
}

fn core_card(work_id: i64, body: &str, importance: f64) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: body.to_string(),
        source: "core".to_string(),
        template_key: String::new(),
        importance,
        linked: vec!["chapter:1".to_string()],
        derived_from: None,
        auto_derived: false,
    }
}

fn draft(work_id: i64, body: &str) -> ModuleQuestion {
    ModuleQuestion {
        work_id,
        body: body.to_string(),
        importance: 0.5,
        linked: vec!["chapter:1".to_string()],
    }
}

/// 不设防的策略：永远放行，丢弃必留痕（谈规则本身时用它）。
fn open_policy() -> ModuleIntakePolicy {
    ModuleIntakePolicy { window_ms: 3_600_000, limit: 100, cooldown_ms: 0, drop_log_interval_ms: 0 }
}

fn drop_records(store: &Store) -> Vec<serde_json::Value> {
    let mut stmt = store
        .conn()
        .prepare("SELECT payload FROM op_log WHERE entity = 'question_intake' AND op = 'drop' ORDER BY seq")
        .unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
    let mut out = Vec::new();
    for row in rows {
        out.push(serde_json::from_str(&row.unwrap()).unwrap());
    }
    out
}

fn accepted(outcome: QuestionIntakeOutcome) -> i64 {
    match outcome {
        QuestionIntakeOutcome::Accepted { card_id } => card_id,
        other => panic!("应当收下：{other:?}"),
    }
}

fn dropped(outcome: QuestionIntakeOutcome) -> QuestionDropReason {
    match outcome {
        QuestionIntakeOutcome::Dropped { reason } => reason,
        other => panic!("应当丢弃：{other:?}"),
    }
}

#[test]
fn a_declared_active_module_asks_and_the_card_carries_its_id() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let grant = ModuleQuestionGrant::ready("mod.map");

    let card_id = accepted(
        store
            .submit_module_question(&grant, draft(work, "这个地点要不要先出现一次？"))
            .unwrap(),
    );
    let card = store.question_card(card_id).unwrap();
    assert_eq!(card.source, "mod.map", "来源必须写明是哪个模块");
    assert_eq!(card.template_key, "", "模块的卡不来自核心模板");
    assert_eq!(card.state, QuestionState::Pending);
    assert_eq!(card.importance, 0.5);
    assert_eq!(card.linked, "[\"chapter:1\"]", "依据锚点照存");

    // 与核心的问题**同一个池、同一个出口**，并且选题出口带着来源
    let selected = store.select_questions_for_chapter(work, chapter, 10).unwrap();
    let row = selected.iter().find(|question| question.card_id == card_id).unwrap();
    assert_eq!(row.source, "mod.map");
    assert_eq!(row.body, "这个地点要不要先出现一次？");
}

#[test]
fn undeclared_and_inactive_modules_are_dropped_and_recorded() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);

    let mut undeclared = ModuleQuestionGrant::ready("mod.map");
    undeclared.may_ask = false;
    assert_eq!(
        dropped(store.submit_module_question_with(&undeclared, draft(work, "问一句"), open_policy()).unwrap()),
        QuestionDropReason::NotDeclared
    );

    let mut inactive = ModuleQuestionGrant::ready("mod.map");
    inactive.active = false;
    assert_eq!(
        dropped(store.submit_module_question_with(&inactive, draft(work, "再问一句"), open_policy()).unwrap()),
        QuestionDropReason::Inactive
    );

    assert!(store.question_cards(work, None).unwrap().is_empty(), "丢弃的不得落成卡");
    let records = drop_records(&store);
    let reasons: Vec<&str> = records.iter().map(|row| row["reason"].as_str().unwrap()).collect();
    assert_eq!(reasons, vec!["not_declared", "inactive"], "丢弃必须留痕");
    assert!(records.iter().all(|row| row["module_id"].as_str() == Some("mod.map")));
}

#[test]
fn a_module_may_not_pretend_to_be_core_nor_use_a_bad_id() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);

    for (module_id, expected) in [
        ("core", QuestionDropReason::SourceReserved),
        ("Core", QuestionDropReason::SourceReserved),
        ("", QuestionDropReason::SourceInvalid),
        ("bad id", QuestionDropReason::SourceInvalid),
        ("有中文", QuestionDropReason::SourceInvalid),
        (&"x".repeat(65), QuestionDropReason::SourceInvalid),
    ] {
        let grant = ModuleQuestionGrant::ready(module_id);
        assert_eq!(
            dropped(store.submit_module_question_with(&grant, draft(work, "冒充一句"), open_policy()).unwrap()),
            expected,
            "模块 id：{module_id:?}"
        );
    }
    assert!(store.question_cards(work, None).unwrap().is_empty());
    assert_eq!(drop_records(&store).len(), 6);
}

#[test]
fn a_question_without_basis_is_dropped() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let grant = ModuleQuestionGrant::ready("mod.map");

    for linked in [vec![], vec![String::new()], vec!["   ".to_string()]] {
        let mut question = draft(work, "没有依据的一句话");
        question.linked = linked;
        assert_eq!(
            dropped(store.submit_module_question_with(&grant, question, open_policy()).unwrap()),
            QuestionDropReason::NoBasis
        );
    }
    assert!(store.question_cards(work, None).unwrap().is_empty());
    assert_eq!(drop_records(&store).len(), 3);
}

#[test]
fn bad_bodies_and_importance_are_errors_not_silent_drops() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let grant = ModuleQuestionGrant::ready("mod.map");

    let mut empty = draft(work, "   ");
    assert_eq!(
        store.submit_module_question_with(&grant, empty.clone(), open_policy()).unwrap_err().code(),
        codes::CARD_BODY_EMPTY
    );
    empty.body = "问一句".into();
    empty.importance = 1.5;
    assert_eq!(
        store.submit_module_question_with(&grant, empty, open_policy()).unwrap_err().code(),
        codes::CARD_IMPORTANCE_OUT_OF_RANGE
    );
    assert!(store.question_cards(work, None).unwrap().is_empty());
}

#[test]
fn the_rate_limit_is_per_module_and_dropped_questions_do_not_pile_up() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let policy = ModuleIntakePolicy { window_ms: 3_600_000, limit: 2, cooldown_ms: 0, drop_log_interval_ms: 0 };

    let one = ModuleQuestionGrant::ready("mod.one");
    for index in 0..2 {
        accepted(
            store
                .submit_module_question_with(&one, draft(work, &format!("第{index}条")), policy)
                .unwrap(),
        );
    }
    assert_eq!(
        dropped(store.submit_module_question_with(&one, draft(work, "第三条"), policy).unwrap()),
        QuestionDropReason::RateLimited
    );
    assert_eq!(store.question_cards(work, None).unwrap().len(), 2, "超限的不排队、不落卡");

    // 另一个模块不受它的账影响
    let two = ModuleQuestionGrant::ready("mod.two");
    accepted(store.submit_module_question_with(&two, draft(work, "另一家的第一条"), policy).unwrap());
    assert_eq!(store.question_cards(work, None).unwrap().len(), 3);

    let last = drop_records(&store).pop().unwrap();
    assert_eq!(last["reason"].as_str(), Some("rate_limited"));
    assert_eq!(last["module_id"].as_str(), Some("mod.one"));
}

#[test]
fn the_cooldown_blocks_a_second_question_even_under_the_limit() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let grant = ModuleQuestionGrant::ready("mod.map");
    let policy = ModuleIntakePolicy { window_ms: 3_600_000, limit: 10, cooldown_ms: 3_600_000, drop_log_interval_ms: 0 };

    accepted(store.submit_module_question_with(&grant, draft(work, "第一条"), policy).unwrap());
    assert_eq!(
        dropped(store.submit_module_question_with(&grant, draft(work, "紧接着第二条"), policy).unwrap()),
        QuestionDropReason::RateLimited
    );
}

#[test]
fn the_window_rolls_over_and_the_module_may_ask_again() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let grant = ModuleQuestionGrant::ready("mod.map");
    // 窗口够宽，保证"紧接着的第二条"一定落在窗口里；睡过窗口之后再试
    let policy = ModuleIntakePolicy { window_ms: 500, limit: 1, cooldown_ms: 0, drop_log_interval_ms: 0 };

    accepted(store.submit_module_question_with(&grant, draft(work, "第一条"), policy).unwrap());
    assert_eq!(
        dropped(store.submit_module_question_with(&grant, draft(work, "窗口内第二条"), policy).unwrap()),
        QuestionDropReason::RateLimited
    );
    std::thread::sleep(Duration::from_millis(550));
    accepted(store.submit_module_question_with(&grant, draft(work, "过窗之后的第一条"), policy).unwrap());
    assert_eq!(store.question_cards(work, None).unwrap().len(), 2);
}

#[test]
fn the_first_drop_is_always_logged_but_repeats_are_throttled() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let grant = ModuleQuestionGrant::ready("mod.map");
    // 丢弃留痕一分钟最多一条
    let policy = ModuleIntakePolicy { window_ms: 3_600_000, limit: 4, cooldown_ms: 0, drop_log_interval_ms: 3_600_000 };

    for _ in 0..3 {
        let mut question = draft(work, "没有依据");
        question.linked.clear();
        dropped(store.submit_module_question_with(&grant, question, policy).unwrap());
    }
    assert_eq!(drop_records(&store).len(), 1, "同一个模块的连续丢弃只记一条");
}

#[test]
fn muting_one_module_leaves_core_and_the_other_module_alone() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    store.create_question_card(&core_card(work, "核心自己的一条", 0.5)).unwrap();
    let one = ModuleQuestionGrant::ready("mod.one");
    let two = ModuleQuestionGrant::ready("mod.two");
    accepted(store.submit_module_question_with(&one, draft(work, "一号模块的一条"), open_policy()).unwrap());
    accepted(store.submit_module_question_with(&two, draft(work, "二号模块的一条"), open_policy()).unwrap());

    store.mute_source("mod.one").unwrap();
    let selected = store.select_questions(work, 10).unwrap();
    let sources: Vec<&str> = selected.iter().map(|question| question.source.as_str()).collect();
    assert!(!sources.contains(&"mod.one"), "被静音的模块不出现");
    assert!(sources.contains(&"core"), "核心不受牵连");
    assert!(sources.contains(&"mod.two"), "别的模块不受牵连");

    store.unmute_source("mod.one").unwrap();
    let again = store.select_questions(work, 10).unwrap();
    assert!(again.iter().any(|question| question.source == "mod.one"), "解除静音后回来");
}

#[test]
fn a_module_question_never_takes_the_proactive_slot_from_core() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    // 核心那张重要度很低；模块那张很高——按引力模块排前面，但"推"必须让核心先开口
    let core_id = store.create_question_card(&core_card(work, "核心的一条", 0.1)).unwrap();
    let grant = ModuleQuestionGrant::ready("mod.map");
    let mut loud = draft(work, "模块的一条");
    loud.importance = 1.0;
    let module_id = accepted(store.submit_module_question_with(&grant, loud, open_policy()).unwrap());

    let quota = PushQuota { per_day: 1, cooldown_minutes: 0 };
    let outcome = store
        .push_question(work, Some(chapter), quota, 2026_09_16, PushReason::NewChapter)
        .unwrap();
    let PushOutcome::Pushed { question } = outcome else { panic!("应当推得出来：{outcome:?}") };
    assert_eq!(question.card_id, core_id, "主动开口的那一次必须优先核心");
    assert_eq!(question.source, "core");
    assert_eq!(store.question_card(module_id).unwrap().state, QuestionState::Pending, "模块的卡留在池里");

    // 核心问完了，下一次才轮到模块
    let quota = PushQuota { per_day: 2, cooldown_minutes: 0 };
    let outcome = store
        .push_question(work, Some(chapter), quota, 2026_09_16, PushReason::Idle)
        .unwrap();
    let PushOutcome::Pushed { question } = outcome else { panic!("第二轮应当推得出来：{outcome:?}") };
    assert_eq!(question.card_id, module_id);
    assert_eq!(question.source, "mod.map");
}

#[test]
fn the_outcome_serializes_to_stable_codes() {
    let accepted = serde_json::to_value(QuestionIntakeOutcome::Accepted { card_id: 7 }).unwrap();
    assert_eq!(accepted, serde_json::json!({ "code": "accepted", "card_id": 7 }));
    let dropped = serde_json::to_value(QuestionIntakeOutcome::Dropped {
        reason: QuestionDropReason::RateLimited,
    })
    .unwrap();
    assert_eq!(dropped, serde_json::json!({ "code": "dropped", "reason": "rate_limited" }));
}
