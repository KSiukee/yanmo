//! 作答验收：**答案落得住、输入方式记对了、状态收得住、坏输入进不来**。
//!
//! 判据全落在可核对的事实上：答案碎片读得回来、卡走到终态、op-log 里有据可查、
//! 正文一个字节没动、已答之后再答被状态机拒、认不出的输入方式一个字节都写不进去。

use yanmo_core::error::codes;
use yanmo_core::model::{InputSource, NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn card(work_id: i64) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: "他为什么不肯把那封信烧了？".to_string(),
        source: "core".to_string(),
        template_key: "chapter.empty_body".to_string(),
        importance: 0.5,
        linked: vec!["chapter:1".to_string()],
        derived_from: None,
        auto_derived: false,
    }
}

/// 建一本书 + 一章，返回（作品 id、章节 id）。
fn seeded(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    (work.id, chapter)
}

/// 一串事件的动作码（`create` / `ask` / `answer` …）。
fn ops(store: &Store, card_id: i64) -> Vec<String> {
    store.card_events(card_id).unwrap().into_iter().map(|event| event.op).collect()
}

#[test]
fn answering_keeps_the_text_and_its_input_source_and_closes_the_card() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.write_body(chapter, "第一章的正文，一个字都不该被作答动到。").unwrap();
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();

    // 文本与输入方式解耦：答案用口述转来的，source 如实记下来
    let answer_id =
        store.record_question_answer(id, "  他怕烧掉就再也认不出自己  ", "voice", "author").unwrap();
    let saved = store.answer_of_question(id).unwrap();

    assert_eq!(saved.id, answer_id);
    assert_eq!(saved.body, "他怕烧掉就再也认不出自己", "答案存的是修剪后的原文");
    assert_eq!(saved.source, "voice");
    assert_eq!(saved.card_id, id, "答案永远查得到自己是答哪张卡的");
    assert_eq!(saved.work_id, work);
    assert_eq!(store.answers_of_question(id).unwrap().len(), 1);

    // 状态收得住：answered 是终态
    assert_eq!(store.question_card(id).unwrap().state, QuestionState::Answered);
    assert_eq!(ops(&store, id), vec!["create", "ask", "answer"]);
    let last = store.card_events(id).unwrap().pop().unwrap();
    assert_eq!((last.from.as_str(), last.to.as_str()), ("asked", "answered"));

    // 只问不写：作答不碰正文一个字
    assert_eq!(
        store.read_body(chapter).unwrap(),
        "第一章的正文，一个字都不该被作答动到。"
    );
}

#[test]
fn answering_a_card_that_was_never_asked_records_the_ask_first() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();

    // 作者在队列里一眼看到就答了——没点开也算答
    store.record_question_answer(id, "因为他答应过她", "typed", "author").unwrap();

    assert_eq!(ops(&store, id), vec!["create", "ask", "answer"], "补一条「问出」再答");
    let asked = store.question_card(id).unwrap();
    assert_eq!(asked.state, QuestionState::Answered);
    assert_eq!(asked.used_count, 1, "新颖度照常消耗——不然库里的账是假的");
    assert!(asked.last_asked_at.is_some(), "问出的时刻也要落下");
    assert_eq!(store.answers_of_question(id).unwrap()[0].source, "typed");
}

#[test]
fn an_unrecognised_input_source_is_refused_and_nothing_is_written() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();

    let err = store.record_question_answer(id, "答案", "telepathy", "author").unwrap_err();
    assert_eq!(err.code(), codes::INPUT_SOURCE_UNKNOWN);

    // 一个字节都没写：卡还在「已问」，答案池是空的，op-log 里只有建卡与问出
    assert_eq!(store.question_card(id).unwrap().state, QuestionState::Asked);
    assert!(store.answers_of_question(id).unwrap().is_empty());
    assert_eq!(ops(&store, id), vec!["create", "ask"]);
}

#[test]
fn an_empty_answer_is_refused_and_nothing_is_written() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();

    let err = store.record_question_answer(id, "   \n  ", "typed", "author").unwrap_err();
    assert_eq!(err.code(), codes::ANSWER_BODY_EMPTY);
    assert_eq!(store.question_card(id).unwrap().state, QuestionState::Asked);
    assert!(store.answers_of_question(id).unwrap().is_empty());
}

#[test]
fn answered_is_terminal_so_a_second_answer_is_refused() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();
    store.record_question_answer(id, "第一版答案", "typed", "author").unwrap();

    let err = store.record_question_answer(id, "第二版答案", "typed", "author").unwrap_err();
    assert_eq!(err.code(), codes::CARD_ILLEGAL_TRANSITION);

    // 原来的答案没被改掉，也没多出一条
    let answers = store.answers_of_question(id).unwrap();
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].body, "第一版答案");
    assert_eq!(ops(&store, id), vec!["create", "ask", "answer"]);
}

#[test]
fn setting_a_card_aside_does_not_make_it_answerable() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();
    store.move_question_card(id, QuestionState::Discarded, "author").unwrap();

    let err = store.record_question_answer(id, "迟来的答案", "typed", "author").unwrap_err();
    assert_eq!(err.code(), codes::CARD_ILLEGAL_TRANSITION);
    assert!(store.answers_of_question(id).unwrap().is_empty());
    assert_eq!(store.question_card(id).unwrap().state, QuestionState::Discarded);
}

#[test]
fn answers_and_inspirations_live_in_the_same_table_without_being_confused() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();
    store.record_question_inspiration(id, "也许信里写的是他母亲的名字", "typed", "author").unwrap();
    store.record_question_answer(id, "他怕烧掉就没人知道那件事了", "mixed", "author").unwrap();

    // 同一张表、两种 frag_kind：各查各的，谁也不串到谁那儿
    let answers = store.answers_of_question(id).unwrap();
    let ideas = store.inspirations_of_question(id).unwrap();
    assert_eq!(answers.len(), 1);
    assert_eq!(ideas.len(), 1);
    assert_eq!(answers[0].body, "他怕烧掉就没人知道那件事了");
    assert_eq!(answers[0].source, "mixed");
    assert_eq!(ideas[0].body, "也许信里写的是他母亲的名字");

    // 答案不是问题卡：它不该出现在候选池里
    assert_eq!(store.question_cards(work, None).unwrap().len(), 1);
}

#[test]
fn a_card_without_an_answer_says_so_instead_of_pretending() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.move_question_card(id, QuestionState::Asked, "pull").unwrap();

    let err = store.answer_of_question(id).unwrap_err();
    assert_eq!(err.code(), codes::ANSWER_NOT_FOUND);
}

#[test]
fn input_source_codes_are_the_only_ones_accepted() {
    // 三个取值都收得下（口述那条链路落地后写入者会换成 voice / mixed）
    for source in InputSource::ALL {
        let (_dir, mut store) = fresh();
        let (work, _chapter) = seeded(&mut store);
        let id = store.create_question_card(&card(work)).unwrap();
        store.record_question_answer(id, "答案", source.as_str(), "author").unwrap();
        assert_eq!(store.answer_of_question(id).unwrap().source, source.as_str());
    }
}
