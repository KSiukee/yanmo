//! 叩问「模板池 + 加权选题 + 偏好学习 + 防自激」验收。
//!
//! 判据都落在**可核对的事实**上：草稿是不是照着书里的东西生成的、同锚点会不会被重复问、
//! 翻看不改库、问出记不记时刻、作者怎么处置就学到什么、静音的那一类会不会真的消失、
//! 派生问题有没有被打折与卡深度。迁移表的每个动作码都必须在这套闭环里**表过态**。

use std::collections::BTreeSet;

use yanmo_core::gravity::{signal_for_action, FeedbackSignal, FragmentLevel};
use yanmo_core::model::{NewQuestionCard, NodeKind, QuestionState, WorkKind, TRANSITIONS};
use yanmo_core::question::TEMPLATES;
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn card(work_id: i64, template_key: &str, anchors: &[&str]) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: format!("draft of {template_key}"),
        source: "core".to_string(),
        template_key: template_key.to_string(),
        importance: 0.5,
        linked: anchors.iter().map(|a| (*a).to_string()).collect(),
        derived_from: None,
        auto_derived: false,
    }
}

/// 建一本有"两章写完（一章留了一句话）、一章没动笔"的书；返回（作品 id, 没动笔那一章的 id）。
fn seeded_book(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let ch1 = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    let ch2 = store.create_node(work.id, None, NodeKind::Chapter, "第二章").unwrap();
    let ch3 = store.create_node(work.id, None, NodeKind::Chapter, "第三章").unwrap();
    store.write_body(ch1, "第一章的正文。").unwrap();
    store.set_node_summary(ch1, "他回了家。").unwrap();
    store.write_body(ch2, "第二章的正文。").unwrap();
    (work.id, ch3)
}

/// 直接读库里那一行（核对"查看不改库"这类事实）。
fn row(store: &Store, id: i64) -> (String, i64, Option<i64>, i64) {
    store
        .conn()
        .query_row(
            "SELECT status, used_count, last_asked_at, updated_at FROM fragments WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
}

#[test]
fn drafts_come_from_the_book_and_are_not_asked_twice() {
    let (_dir, mut store) = fresh();
    let (work, empty_chapter) = seeded_book(&mut store);

    let drafts = store.question_drafts(work).unwrap();
    let keys: Vec<&str> = drafts.iter().map(|d| d.template_key).collect();
    assert_eq!(
        keys,
        vec!["chapter.empty_body", "chapter.missing_summary", "review.recent_chapter"],
        "该问的三件事：还没动笔的章 / 没留一句话的章 / 刚写完那一章的承接"
    );
    assert_eq!(drafts[0].slots["chapter"], "第三章", "槽位取渲染后的章名");
    assert_eq!(drafts[0].anchors, vec![format!("chapter:{empty_chapter}")]);

    // 落成卡（界面渲染句子 + 建卡；这里用占位正文），锚点带着走
    for draft in &drafts {
        let anchors: Vec<&str> = draft.anchors.iter().map(String::as_str).collect();
        store
            .create_question_card(&card(work, draft.template_key, &anchors))
            .unwrap();
    }
    assert!(
        store.question_drafts(work).unwrap().is_empty(),
        "同一个锚点上同一条模板，问过就不该再生成"
    );

    // 舍弃过的那条也不再生成（冷却库里可以捞回，但不该又冒出一张重复的）
    let asked = store.question_cards(work, None).unwrap();
    store
        .move_question_card(asked[0].id, QuestionState::Discarded, "author")
        .unwrap();
    assert!(store.question_drafts(work).unwrap().is_empty());
}

#[test]
fn selection_is_read_only_and_puts_never_asked_first() {
    let (_dir, mut store) = fresh();
    let (work, _empty_chapter) = seeded_book(&mut store);
    let key = "chapter.empty_body";

    // 一张从没问过；两张问过一次又捞回（一张刚问过、一张两个月前问过）
    let fresh_card = store.create_question_card(&card(work, key, &["chapter:3"])).unwrap();
    let recent = store.create_question_card(&card(work, key, &["chapter:4"])).unwrap();
    let old = store.create_question_card(&card(work, key, &["chapter:5"])).unwrap();
    for id in [recent, old] {
        store.move_question_card(id, QuestionState::Asked, "push").unwrap();
        store.move_question_card(id, QuestionState::Discarded, "author").unwrap();
        store.move_question_card(id, QuestionState::Pending, "retrieve").unwrap();
    }
    let now: i64 = store
        .conn()
        .query_row("SELECT last_asked_at FROM fragments WHERE id = ?1", [old], |r| r.get(0))
        .unwrap();
    let long_ago = now - 60 * 86_400_000;
    store
        .conn()
        .execute(&format!("UPDATE fragments SET last_asked_at = {long_ago} WHERE id = {old}"), [])
        .unwrap();

    let before: Vec<_> = [fresh_card, recent, old].iter().map(|id| row(&store, *id)).collect();
    let picked = store.select_questions(work, 10).unwrap();
    let after: Vec<_> = [fresh_card, recent, old].iter().map(|id| row(&store, *id)).collect();
    assert_eq!(before, after, "选题是只读的：看多少问题都不该改库");

    let order: Vec<i64> = picked.iter().map(|q| q.card_id).collect();
    assert_eq!(order, vec![fresh_card, old, recent], "从没问过的最前，问过越久越靠前");
    let by_id = |id: i64| picked.iter().find(|q| q.card_id == id).unwrap();
    assert_eq!(by_id(fresh_card).gravity.novelty, 1.0, "没问过的新颖度是满的");
    assert!(by_id(old).gravity.novelty > by_id(recent).gravity.novelty, "时间让新颖度回升");
    assert!(by_id(recent).gravity.novelty < 1.0, "问过的回不到满分");
    assert_eq!(by_id(fresh_card).gravity.level, FragmentLevel::Short, "问得少就是短期档");
}

#[test]
fn asking_records_the_moment_and_takes_the_card_out_of_the_pool() {
    let (_dir, mut store) = fresh();
    let (work, _empty_chapter) = seeded_book(&mut store);
    let id = store.create_question_card(&card(work, "chapter.empty_body", &["chapter:3"])).unwrap();

    let from = store.move_question_card(id, QuestionState::Asked, "push").unwrap();
    assert_eq!(from, QuestionState::Pending);
    let asked = store.question_card(id).unwrap();
    assert_eq!(asked.used_count, 1, "问出一次，账加一");
    assert!(asked.last_asked_at.is_some(), "问出要记时刻——新颖度随时间回收靠它");
    assert!(store.select_questions(work, 10).unwrap().is_empty(), "问过的卡不在候选池里");
}

#[test]
fn praise_teaches_without_touching_the_state() {
    let (_dir, mut store) = fresh();
    let (work, _empty_chapter) = seeded_book(&mut store);
    let key = "chapter.empty_body";
    let id = store.create_question_card(&card(work, key, &["chapter:3"])).unwrap();

    store.praise_question_card(id, "author").unwrap();
    let praised = store.question_card(id).unwrap();
    assert_eq!(praised.state, QuestionState::Pending, "评价与处置是两件事：状态一个字节都不动");
    let learned = store.template_weight(key).unwrap();
    assert!((learned.weight - 1.5).abs() < 1e-12 && learned.positives == 1, "{learned:?}");
    assert!(
        store.card_events(id).unwrap().iter().any(|e| e.op == "praise"),
        "夸过也要留痕"
    );

    // 作者自己写的问题（没有模板键）没有同类可教：不报错、也不动任何权重
    let manual = store
        .create_question_card(&NewQuestionCard {
            template_key: String::new(),
            body: "作者自己写的问题".to_string(),
            ..card(work, "", &[])
        })
        .unwrap();
    store.praise_question_card(manual, "author").unwrap();
    assert_eq!(store.template_weights().unwrap().len(), 1, "只有那一条模板学到了东西");
}

#[test]
fn disposition_teaches_the_template_automatically() {
    let (_dir, mut store) = fresh();
    let (work, _empty_chapter) = seeded_book(&mut store);
    let key = "chapter.empty_body";

    // 作答 = 中性：权重一动不动
    let answered = store.create_question_card(&card(work, key, &[])).unwrap();
    store.move_question_card(answered, QuestionState::Asked, "push").unwrap();
    store.move_question_card(answered, QuestionState::Answered, "author").unwrap();
    assert_eq!(store.template_weight(key).unwrap().weight, 1.0, "作答不该改变偏好");

    // 舍弃 = 负样本：降权（不用谁再喊一声"顺便教一下"，处置本身就是教学）
    let discarded = store.create_question_card(&card(work, key, &[])).unwrap();
    store.move_question_card(discarded, QuestionState::Asked, "push").unwrap();
    store.move_question_card(discarded, QuestionState::Discarded, "author").unwrap();
    let after = store.template_weight(key).unwrap();
    assert!((after.weight - 0.6).abs() < 1e-12 && after.negatives == 1, "{after:?}");

    // 静音 = 强负：这一类整体停用（不排到后面，是不出现）
    let muted = store.create_question_card(&card(work, key, &[])).unwrap();
    store.move_question_card(muted, QuestionState::Muted, "author").unwrap();
    assert!(!store.template_weight(key).unwrap().enabled, "静音之后该类停用");
    let alive = store.create_question_card(&card(work, key, &[])).unwrap();
    let picked = store.select_questions(work, 10).unwrap();
    assert!(
        picked.iter().all(|q| q.card_id != alive && q.template_key != key),
        "静音的那一类不该出现在候选里"
    );

    // 静音**可以撤销**（没有出口就成了死路）：作者显式解除那一类
    let lifted = store.unmute_template(key).unwrap();
    assert!(lifted.enabled && (lifted.weight - 1.0).abs() < 1e-12, "{lifted:?}");
    assert_eq!(lifted.negatives, 2, "一次舍弃 + 一次静音，教过的记录都留着");
    assert!(
        store.select_questions(work, 10).unwrap().iter().any(|q| q.template_key == key),
        "解除静音之后这一类的问题又回到候选池"
    );
}

#[test]
fn auto_derived_questions_are_discounted_and_capped() {
    let (_dir, mut store) = fresh();
    let (work, _empty_chapter) = seeded_book(&mut store);
    let key = "review.recent_chapter";

    // 手动派生：不受链深限制
    let root = store.create_question_card(&card(work, key, &[])).unwrap();
    let manual = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(root),
            auto_derived: false,
            ..card(work, key, &[])
        })
        .unwrap();
    assert_eq!(store.question_card(manual).unwrap().derived_from, Some(root));

    // 自动派生：链深一到上限就拒绝（防"问题→灵感→问题"无限套娃）
    let mut chain = Vec::new();
    let mut parent = root;
    for _ in 0..2 {
        let id = store
            .create_question_card(&NewQuestionCard {
                derived_from: Some(parent),
                auto_derived: true,
                ..card(work, key, &[])
            })
            .unwrap();
        chain.push(id);
        parent = id;
    }
    let err = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(parent),
            auto_derived: true,
            ..card(work, key, &[])
        })
        .unwrap_err();
    assert_eq!(err.code(), "card.derivation_too_deep");
    // 作者手动顺着同一条链再问：不受此限
    store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(parent),
            auto_derived: false,
            ..card(work, key, &[])
        })
        .unwrap();
    // "自动派生"却不说来源：说不通，当场拒
    let err = store
        .create_question_card(&NewQuestionCard {
            auto_derived: true,
            ..card(work, key, &[])
        })
        .unwrap_err();
    assert_eq!(err.code(), "card.auto_derived_needs_source");

    // 折扣：原生不打折，自动派生的打对折——**只影响排序，不是禁用**（它照样在候选里）
    let picked = store.select_questions(work, 100).unwrap();
    let discount = |id: i64| {
        picked.iter().find(|q| q.card_id == id).expect("该在候选里").gravity.derived_discount
    };
    assert_eq!(discount(root), 1.0, "原生问题不打折");
    assert_eq!(discount(chain[0]), 0.5, "自动派生的打折");
    assert!(picked.iter().any(|q| q.card_id == chain[0]), "派生问题照样在候选里");
}

#[test]
fn every_action_in_the_state_machine_has_an_explicit_lesson() {
    // 期望表：动作码 → 教什么（None = 中性，不改变偏好）
    let expected: &[(&str, Option<FeedbackSignal>)] = &[
        ("ask", None),
        ("answer", None),
        ("defer", None),
        ("discard", Some(FeedbackSignal::Discarded)),
        ("mute", Some(FeedbackSignal::Muted)),
        ("requeue", None),
        ("retrieve", None),
        ("unmute", None),
    ];
    for (action, want) in expected {
        assert_eq!(signal_for_action(action), *want, "动作码 {action} 的信号变了");
    }
    // 双向对表：迁移表里的动作都表过态，映射里也不许有迁移表里没有的动作
    let actions: BTreeSet<&str> = TRANSITIONS.iter().map(|t| t.action).collect();
    for action in &actions {
        assert!(
            expected.iter().any(|(a, _)| a == action),
            "迁移表里的动作 {action} 没在偏好映射里表态——处置会被静默地学不到"
        );
    }
    for (action, _) in expected {
        assert!(actions.contains(action), "偏好映射里的 {action} 不是迁移表里的动作");
    }
}

#[test]
fn every_template_in_the_pool_is_reachable_by_the_engine() {
    // 模板池里的每一条都要能被"生成 → 建卡 → 排序"这条路走到（不许有只写在纸上的模板）
    let (_dir, mut store) = fresh();
    let (work, _empty_chapter) = seeded_book(&mut store);
    let mut seen = BTreeSet::new();
    for t in TEMPLATES {
        let id = store.create_question_card(&card(work, t.key, &[t.key])).unwrap();
        seen.insert(store.question_card(id).unwrap().template_key);
    }
    let picked = store.select_questions(work, 100).unwrap();
    for t in TEMPLATES {
        assert!(picked.iter().any(|q| q.template_key == t.key), "{} 排不进候选池", t.key);
    }
    assert_eq!(seen.len(), TEMPLATES.len());
}

#[test]
fn rerunning_v9_keeps_the_learned_weights() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let work;
    let card_id;
    {
        let mut store = Store::open(&path).unwrap();
        work = seeded_book(&mut store).0;
        card_id = store.create_question_card(&card(work, "chapter.empty_body", &[])).unwrap();
        store.praise_question_card(card_id, "author").unwrap();
        store.move_question_card(card_id, QuestionState::Asked, "push").unwrap();
    }
    {
        let conn = yanmo_core::db::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 8).unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert!((store.template_weight("chapter.empty_body").unwrap().weight - 1.5).abs() < 1e-12);
    let card = store.question_card(card_id).unwrap();
    assert!(card.last_asked_at.is_some(), "重跑 v9 不该弄丢「上次问出的时刻」");
    assert_eq!(card.used_count, 1);
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
}

/// 模式 A「跟着这一章走」：**锚点落在这一章的抬到最前**，其余照旧按引力跟着——一条都不少。
///
/// 只抬当前章、不去猜"下一章是哪一章"：作者换到哪一章，那一章的问题就自然浮上来。
#[test]
fn following_a_chapter_lifts_just_its_own_questions_to_the_front() {
    let (_dir, mut store) = fresh();
    let (work, empty_chapter) = seeded_book(&mut store);
    let here = store
        .create_question_card(&card(
            work,
            "chapter.empty_body",
            &[&format!("chapter:{empty_chapter}")],
        ))
        .unwrap();
    let elsewhere =
        store.create_question_card(&card(work, "review.recent_chapter", &["chapter:1"])).unwrap();

    let plain = store.select_questions(work, 10).unwrap();
    let ordered = store.select_questions_for_chapter(work, empty_chapter, 10).unwrap();

    assert_eq!(ordered.len(), plain.len(), "只是抬顺序，不是把别的筛掉");
    assert_eq!(ordered[0].card_id, here, "这一章的问题排最前");
    assert_eq!(
        ordered[0].anchors,
        vec![format!("chapter:{empty_chapter}")],
        "锚点要带给界面——界面靠它认这条问的是不是这一章"
    );
    assert!(ordered.iter().any(|q| q.card_id == elsewhere), "别的问题照样在列表里");

    // 这一章没有专属问题时，顺序**原样不动**（不猜、不筛）
    let untouched = store.select_questions_for_chapter(work, 999, 10).unwrap();
    assert_eq!(untouched, plain);
}

/// 同类不扎堆：一屏里同一类（模板）只摆一条，其余排到别的类后面——**一条都不丢**。
///
/// 这条是为"书里状态重复"准备的：十来个空章会产出十来条只差章名的问题，
/// 一屏全是同一件事，作者会以为机制只会问这一句。
#[test]
fn one_screen_does_not_pile_up_the_same_kind_of_question() {
    let (_dir, mut store) = fresh();
    let (work, _empty) = seeded_book(&mut store);
    // 五条同一模板（只差锚点），两条别的模板
    for chapter in 1..=5 {
        store
            .create_question_card(&card(work, "chapter.empty_body", &[&format!("chapter:{chapter}")]))
            .unwrap();
    }
    let other_a = store.create_question_card(&card(work, "review.recent_chapter", &["chapter:1"])).unwrap();
    let other_b = store.create_question_card(&card(work, "rhythm.length_swing", &[])).unwrap();

    let picked = store.select_questions(work, 3).unwrap();
    assert_eq!(picked.len(), 3);
    assert_eq!(
        picked.iter().filter(|q| q.template_key == "chapter.empty_body").count(),
        1,
        "同一类只摆一条：{:?}",
        picked.iter().map(|q| q.template_key.as_str()).collect::<Vec<_>>()
    );
    assert!(
        picked.iter().any(|q| q.card_id == other_a) && picked.iter().any(|q| q.card_id == other_b),
        "别的类不该被同类挤掉"
    );

    // 一条都不丢：要得多的时候，同类的其余几条照样排得出来（只是排在后面）
    let all = store.select_questions(work, 10).unwrap();
    assert_eq!(all.len(), 7, "七条都在池子里");
    assert_eq!(all.iter().filter(|q| q.template_key == "chapter.empty_body").count(), 5);
    assert_eq!(store.count_pending_questions(work).unwrap(), 7, "池子里的总数不受一屏上限影响");
}
