//! 作答验收：**答案落得住、输入方式记对了、状态收得住、坏输入进不来**。
//!
//! 判据全落在可核对的事实上：答案碎片读得回来、卡走到终态、op-log 里有据可查、
//! 正文一个字节没动、已答之后再答被状态机拒、认不出的输入方式一个字节都写不进去。
//! 外加**先问后排版**（一轮一次落）那几条：顺序由调用方定、改过的字回写并留痕、
//! 半轮落不下去时一条都不写。

use yanmo_core::error::codes;
use yanmo_core::model::{InputSource, NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::store::{RoundItem, Store};

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

/// 落进正文：**只留痕，不写正文**——正文那一笔是界面那条编辑路写的。
#[test]
fn landing_marks_the_answer_and_leaves_a_trail_without_touching_the_prose() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.write_body(chapter, "第一章的正文，本命令一个字都不该动它。").unwrap();
    let id = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(id, "他怕烧掉就认不出自己", "typed", "author").unwrap();
    let before = store.read_body(chapter).unwrap();

    let receipt = store.mark_answer_landed(id, chapter, "body", "", "author").unwrap();
    let answer_id = receipt.answer_ids[0];

    let saved = store.answer_of_question(id).unwrap();
    assert_eq!(saved.id, answer_id);
    assert_eq!(saved.status, "landed", "落过正文的答案看得出来");
    let ops: Vec<String> = store
        .card_events(answer_id)
        .unwrap()
        .into_iter()
        .map(|event| event.op)
        .collect();
    assert_eq!(ops, vec!["create", "land"], "落一次记一条");
    let payload: String = store
        .conn()
        .query_row(
            "SELECT payload FROM op_log WHERE entity = 'fragments' AND entity_id = ?1 AND op = 'land'",
            [answer_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(payload.contains(&format!("\"node_id\":{chapter}")), "留痕要说清落到哪一章：{payload}");
    assert_eq!(store.read_body(chapter).unwrap(), before, "只留痕：正文一个字节没动");
}

#[test]
fn landing_twice_is_allowed_and_records_each_time() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(id, "同一句话再放一次也无妨", "typed", "author").unwrap();

    store.mark_answer_landed(id, chapter, "body", "", "author").unwrap();
    store.mark_answer_landed(id, chapter, "body", "", "author").unwrap();

    let ops: Vec<String> = store
        .card_events(store.answer_of_question(id).unwrap().id)
        .unwrap()
        .into_iter()
        .map(|event| event.op)
        .collect();
    assert_eq!(ops, vec!["create", "land", "land"], "每落一次都有据可查");
}

#[test]
fn landing_refuses_a_node_that_holds_no_body_or_belongs_to_another_book() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let volume = store.create_node(work, None, NodeKind::Volume, "第一卷").unwrap();
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    let stranger = store.create_node(other.id, None, NodeKind::Chapter, "别人的第一章").unwrap();
    let id = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(id, "答案", "typed", "author").unwrap();

    for (node, expected, why) in [
        (volume, codes::ANSWER_LAND_NODE_INVALID, "卷不承载正文"),
        (stranger, codes::ANSWER_LAND_NODE_INVALID, "别的作品的章"),
        (9999, codes::NODE_GONE, "不存在的节点"),
    ] {
        let err = store.mark_answer_landed(id, node, "body", "", "author").unwrap_err();
        assert_eq!(err.code(), expected, "{why}");
    }
    assert_eq!(
        store.answer_of_question(id).unwrap().status,
        "pending",
        "被拒之后一个字节都没写：答案还躺在答案池里"
    );
    assert_eq!(store.read_body(chapter).unwrap(), "", "也没往任何一章里塞字");
}

#[test]
fn landing_without_an_answer_says_so() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();

    let err = store.mark_answer_landed(id, chapter, "body", "", "author").unwrap_err();
    assert_eq!(err.code(), codes::ANSWER_NOT_FOUND);
}

/// 一轮落章（先问后排版）：**顺序由调用方定、改过的字回写答案池、正文仍是一个字节不动**。
#[test]
fn a_round_lands_in_the_given_order_and_writes_back_edits() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let first = store.create_question_card(&card(work)).unwrap();
    let second = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(first, "第一段", "typed", "author").unwrap();
    store.record_question_answer(second, "第二段", "typed", "author").unwrap();

    // 作者在托盘里把顺序倒过来了，还顺手改了第二条的字（首尾空白也该修剪掉）
    let landed = store
        .apply_answer_round(
            work,
            chapter,
            &[
                RoundItem { card_id: second, body: "  第二段（改了字）  ".to_string(), target: String::new(), title: String::new() },
                RoundItem { card_id: first, body: "第一段".to_string(), target: String::new(), title: String::new() },
            ],
            "author",
        )
        .unwrap();

    let edited = store.answer_of_question(second).unwrap();
    let untouched = store.answer_of_question(first).unwrap();
    assert_eq!(landed.answer_ids, vec![edited.id, untouched.id], "落下的顺序就是给的那个顺序");
    assert!(landed.scene_ids.is_empty() && landed.outline.is_none(), "这一轮只落正文");
    assert_eq!(edited.body, "第二段（改了字）", "改过的字回写了答案池");
    assert_eq!(edited.status, "landed");
    assert_eq!(untouched.body, "第一段");

    // 留痕：改了的那条 create → amend → land，没改的只有 create → land
    let ops = |id: i64| -> Vec<String> {
        store.card_events(id).unwrap().into_iter().map(|event| event.op).collect()
    };
    assert_eq!(ops(edited.id), vec!["create", "amend", "land"]);
    assert_eq!(ops(untouched.id), vec!["create", "land"]);
    let payload: String = store
        .conn()
        .query_row(
            "SELECT payload FROM op_log WHERE entity = 'fragments' AND entity_id = ?1 AND op = 'amend'",
            [edited.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        payload.contains(&format!("\"chars_before\":{}", "第二段".chars().count()))
            && payload.contains(&format!("\"chars\":{}", "第二段（改了字）".chars().count())),
        "改前改后各有几个字要留得下来：{payload}"
    );

    // 只留痕：正文那几段字是界面插进编辑会话的，核心一个字节都不写
    assert_eq!(store.read_body(chapter).unwrap(), "");
}

/// 一轮里有一条不对，**整轮都不写**（半轮落下去比整轮不落更难查）。
#[test]
fn a_round_with_one_bad_item_writes_nothing_at_all() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let good = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(good, "这一条没问题", "typed", "author").unwrap();
    let unanswered = store.create_question_card(&card(work)).unwrap();
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    let stranger = store.create_question_card(&card(other.id)).unwrap();
    store.record_question_answer(stranger, "别人家的答案", "typed", "author").unwrap();
    let volume = store.create_node(work, None, NodeKind::Volume, "第一卷").unwrap();

    let one = |card_id: i64, body: &str| {
        vec![RoundItem {
            card_id,
            body: body.to_string(),
            target: String::new(),
            title: String::new(),
        }]
    };
    let cases: [(Vec<RoundItem>, &str, &str); 4] = [
        (vec![], codes::ROUND_EMPTY, "空轮"),
        (one(good, "  "), codes::ANSWER_BODY_EMPTY, "空答案"),
        (one(unanswered, "没答过就落"), codes::ANSWER_NOT_FOUND, "没答案的卡"),
        (one(stranger, "别人家的答案"), codes::ROUND_CARD_FOREIGN, "别的书的卡"),
    ];
    for (items, expected, why) in cases {
        let err = store.apply_answer_round(work, chapter, &items, "author").unwrap_err();
        assert_eq!(err.code(), expected, "{why}");
    }
    // 落点不是正文节点：同一条码（这一段放不了）
    let err = store
        .apply_answer_round(work, volume, &one(good, "好答案"), "author")
        .unwrap_err();
    assert_eq!(err.code(), codes::ANSWER_LAND_NODE_INVALID);

    // 一路被拒之后：那条好答案还躺在答案池里，一个字节都没写
    assert_eq!(store.answer_of_question(good).unwrap().status, "pending");
    assert_eq!(store.answer_of_question(good).unwrap().body, "这一条没问题");
    assert_eq!(
        store.card_events(store.answer_of_question(good).unwrap().id).unwrap().len(),
        1,
        "只有建卡那一条痕"
    );
}

/// 落点三档：章纲（合并成一行）、场景卡（新建一张、正文就是答案）、正文（一个字都不写）。
#[test]
fn answers_land_where_the_author_says() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.set_node_summary(chapter, "他回了家。").unwrap();
    let pov = store.create_question_card(&card(work)).unwrap();
    let scene = store.create_question_card(&card(work)).unwrap();
    let prose = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(pov, "第三人称，跟着林望", "typed", "author").unwrap();
    store.record_question_answer(scene, "雨夜，码头，他等一个不会来的人", "typed", "author").unwrap();
    store.record_question_answer(prose, "他站在门口，没敢敲门。", "typed", "author").unwrap();

    let done = store
        .apply_answer_round(
            work,
            chapter,
            &[
                RoundItem { card_id: pov, body: "第三人称，跟着林望".to_string(), target: "outline".to_string(), title: String::new() },
                RoundItem { card_id: scene, body: "雨夜，码头，他等一个不会来的人".to_string(), target: "scene".to_string(), title: "码头".to_string() },
                RoundItem { card_id: prose, body: "他站在门口，没敢敲门。".to_string(), target: String::new(), title: String::new() },
            ],
            "author",
        )
        .unwrap();

    // ① 章纲：原有那句话留着，新的接在后面（一行，用「；」）
    let summary: String = store
        .conn()
        .query_row("SELECT summary FROM nodes WHERE id = ?1", [chapter], |r| r.get(0))
        .unwrap();
    assert_eq!(summary, "他回了家。；第三人称，跟着林望");
    assert_eq!(done.outline.as_deref(), Some(summary.as_str()), "回执要给界面库里的真值");

    // ② 场景卡：这一章下面新建一张，名字是作者起的，正文就是那条答案
    assert_eq!(done.scene_ids.len(), 1);
    let scene_id = done.scene_ids[0];
    let (kind, title, parent): (String, String, Option<i64>) = store
        .conn()
        .query_row(
            "SELECT node_kind, title, parent_id FROM nodes WHERE id = ?1",
            [scene_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (kind.as_str(), title.as_str(), parent),
        ("scene", "码头", Some(chapter)),
        "场景卡挂在**这一章**下面，名字是作者起的"
    );
    assert_eq!(store.read_body(scene_id).unwrap(), "雨夜，码头，他等一个不会来的人");

    // ③ 正文那一条：核心一个字都不写（正文由界面插进编辑会话）
    assert_eq!(store.read_body(chapter).unwrap(), "");
    assert_eq!(done.answer_ids.len(), 3, "三条都算落了");

    // 每条痕都写清落到哪儿
    let payload: String = store
        .conn()
        .query_row(
            "SELECT payload FROM op_log WHERE entity = 'fragments' AND entity_id = ?1 AND op = 'land'",
            [store.answer_of_question(scene).unwrap().id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(payload.contains("\"target\":\"scene\""), "{payload}");
    assert!(payload.contains(&format!("\"scene_id\":{scene_id}")), "场景卡要记下新卡的 id：{payload}");
}

/// 同一句话落两回：章纲里不该出现两遍（别的落法不受影响——"再放一次"是作者的自由）。
#[test]
fn landing_the_same_line_twice_does_not_double_it_in_the_outline() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let id = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(id, "第一场戏在码头", "typed", "author").unwrap();
    let once = |store: &mut Store| {
        store
            .mark_answer_landed(id, chapter, "outline", "", "author")
            .unwrap()
            .outline
            .unwrap()
    };
    assert_eq!(once(&mut store), "第一场戏在码头");
    assert_eq!(once(&mut store), "第一场戏在码头", "同一句不重复接");
    assert_eq!(store.answer_of_question(id).unwrap().status, "landed");
}

/// 认不出的落点当场拒：整轮都不写，章纲与场景卡一个都不动。
#[test]
fn an_unknown_target_is_refused_before_anything_is_written() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let good = store.create_question_card(&card(work)).unwrap();
    let bad = store.create_question_card(&card(work)).unwrap();
    store.record_question_answer(good, "这一条要落章纲", "typed", "author").unwrap();
    store.record_question_answer(bad, "这一条落点写错了", "typed", "author").unwrap();

    let err = store
        .apply_answer_round(
            work,
            chapter,
            &[
                RoundItem { card_id: good, body: "这一条要落章纲".to_string(), target: "outline".to_string(), title: String::new() },
                RoundItem { card_id: bad, body: "这一条落点写错了".to_string(), target: "telepathy".to_string(), title: String::new() },
            ],
            "author",
        )
        .unwrap_err();
    assert_eq!(err.code(), codes::ANSWER_TARGET_UNKNOWN);

    let summary: String = store
        .conn()
        .query_row("SELECT summary FROM nodes WHERE id = ?1", [chapter], |r| r.get(0))
        .unwrap();
    assert_eq!(summary, "", "半轮落不下去：章纲一个字都没写");
    assert_eq!(store.answer_of_question(good).unwrap().status, "pending");
    assert_eq!(store.question_cards(work, None).unwrap().len(), 2, "也没多出场景卡");
}
