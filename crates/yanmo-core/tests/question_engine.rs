//! 叩问「模板池 + 加权选题 + 偏好学习 + 防自激」验收。
//!
//! 判据都落在**可核对的事实**上：草稿是不是照着书里的东西生成的、同锚点会不会被重复问、
//! 翻看不改库、问出记不记时刻、作者怎么处置就学到什么、静音的那一类会不会真的消失、
//! 派生问题有没有被打折与卡深度。迁移表的每个动作码都必须在这套闭环里**表过态**。

use std::collections::BTreeSet;

use yanmo_core::gravity::{signal_for_action, FeedbackSignal, FragmentLevel};
use yanmo_core::model::{
    EntityKind, ForeshadowState, FragmentKind, NewEntityCard, NewForeshadow, NewQuestionCard,
    NodeKind, QuestionState, WorkKind, TRANSITIONS,
};
use yanmo_core::outline::{IssueRule, UNCOLLECTED_CHAPTERS};
use yanmo_core::question::{ElementKind, TEMPLATES, STALLED_CAST_CHAPTERS};
use yanmo_core::store::{NewFragment, Store, SPARK_ELEMENTS};

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

// ── 三路真实信号接进叩问：伏笔超期（A）与灵感碎片引力（B） ────────────────────

/// 一本有 `chapters` 章的书；返回（作品 id, 这些章的节点 id 按顺序）。
///
/// ⚠️ 不给 `list_nodes()` 的下标：`create_work` 会先建一个根卷节点，卷不承载正文、
/// 不算"章"——伏笔的埋点换算成下标时正是拿"承载正文的节点"这一份清单当坐标。
fn book_with_chapters(store: &mut Store, chapters: usize) -> (i64, Vec<i64>) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let mut ids = Vec::new();
    for index in 1..=chapters {
        ids.push(
            store
                .create_node(work.id, None, NodeKind::Chapter, &format!("第{index}章"))
                .unwrap(),
        );
    }
    (work.id, ids)
}

/// 一张人物卡（出生时名字不许空——`clean` 挡住了；要造"名字空着"那种坏数据
/// 只能在库里直接抹，见下面那条测试）。
fn person(store: &mut Store, work: i64, name: &str) -> i64 {
    store
        .create_entity_card(
            &NewEntityCard {
                work_id: work,
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

/// 人物线的事实读数：谁、静了多少章（没人出场过的是 `None`）。
fn quiet_chapters(store: &Store, work: i64, entity: i64) -> Option<i64> {
    let elements = store.writing_elements(work).unwrap();
    elements
        .characters
        .iter()
        .find(|person| person.id == entity)
        .unwrap_or_else(|| panic!("人物 {entity} 该在要素清单里"))
        .chapters_since
}

fn planted_foreshadow(store: &mut Store, work: i64, body: &str, planted: Option<i64>) -> i64 {
    store
        .create_foreshadow(
            &NewForeshadow {
                work_id: work,
                body: body.to_string(),
                planted_node: planted,
                note: String::new(),
            },
            "test",
        )
        .unwrap()
}

fn jot(store: &mut Store, work: i64, body: &str, anchors: Vec<String>) -> i64 {
    store
        .create_fragment(
            &NewFragment {
                work_id: work,
                kind: FragmentKind::Idea,
                body: body.to_string(),
                source: "typed".to_string(),
                anchors,
            },
            "test",
        )
        .unwrap()
}

/// 同一本书上，叩问与大纲体检对"该收了"必须**同时**成立——**夹着场景卡也不能打架**。
///
/// 这条盯的是一个真实分歧：叩问那份章节清单含场景卡（章节类规则要用它），而大纲体检数的
/// 是"正文单位"（不含场景卡）。同一个 20 章阈值架在两份清单上，夹了场景卡的两边就会错开——
/// 旧写法按含场景卡的清单数，于是"差一章 + 两张场景卡"会被算成过了阈值：叩问说该收了、
/// 体检说还没到。现在"隔了多少章"由存储层按体检那侧的口径算好，两边同一个数。
#[test]
fn the_overdue_verdict_agrees_with_the_outline_check_even_with_scene_cards() {
    let threshold = UNCOLLECTED_CHAPTERS.max(1) as usize;
    let (_dir, mut store) = fresh();
    // 章距 = 章数 − 1：`threshold` 章 ⇒ 差一章
    let (work, chapters) = book_with_chapters(&mut store, threshold);
    // 埋点那一章下面挂两张场景卡（场景卡承载正文、会进叩问那份清单，但不算"章"）
    for index in 0..2 {
        store
            .create_node(work, Some(chapters[0]), NodeKind::Scene, &format!("场景{index}"))
            .unwrap();
    }
    let id = planted_foreshadow(&mut store, work, "还不到时候", Some(chapters[0]));

    let asks = |store: &Store| {
        store
            .question_drafts(work)
            .unwrap()
            .into_iter()
            .any(|draft| draft.template_key == "foreshadow.overdue")
    };
    let flags = |store: &Store| {
        store
            .outline_issues(work)
            .unwrap()
            .into_iter()
            .any(|issue| issue.rule == IssueRule::ForeshadowUncollected)
    };

    assert!(!asks(&store), "章数还差一章：场景卡不该把它顶过阈值");
    assert!(!flags(&store), "大纲体检也该说\"还没到\"——两边必须同一个口径");

    // 再加一章：章距刚好踩线 ⇒ 两边同时说"该收了"
    store.create_node(work, None, NodeKind::Chapter, "再多一章").unwrap();
    assert!(asks(&store), "踩线就该问：{id}");
    assert!(flags(&store), "体检也该说该收了");
}

/// 伏笔那条线：埋点换成**阅读顺序里的下标**，"隔了多少章"与大纲体检同一个阈值。
#[test]
fn an_overdue_foreshadow_becomes_a_draft_and_a_fresh_one_does_not() {
    let (_dir, mut store) = fresh();
    let threshold = UNCOLLECTED_CHAPTERS.max(1) as usize;
    let (work, chapters) = book_with_chapters(&mut store, threshold + 1);
    let first = chapters[0]; // 下标 0 → 隔了 threshold 章（够久）
    let second = chapters[1]; // 下标 1 → 差一章

    let overdue = planted_foreshadow(&mut store, work, "老张的怀表", Some(first));
    planted_foreshadow(&mut store, work, "那封信", Some(second));
    planted_foreshadow(&mut store, work, "没记埋点", None);

    let drafts = store.question_drafts(work).unwrap();
    let overdue_drafts: Vec<_> =
        drafts.iter().filter(|d| d.template_key == "foreshadow.overdue").collect();
    assert_eq!(overdue_drafts.len(), 1, "只有够久的那一条该问：{drafts:?}");
    assert_eq!(overdue_drafts[0].anchors, vec![format!("foreshadow:{overdue}")]);
    assert_eq!(overdue_drafts[0].slots["foreshadow"], "老张的怀表");
    assert_eq!(overdue_drafts[0].slots["chapters"], threshold.to_string());
    assert_eq!(overdue_drafts[0].element, ElementKind::Foreshadow);

    // 收掉它：不再问（"埋着"才问，与大纲体检同一个口径）
    store.move_foreshadow(overdue, ForeshadowState::Collected, Some(first), "test").unwrap();
    let after = store.question_drafts(work).unwrap();
    assert!(
        !after.iter().any(|d| d.template_key == "foreshadow.overdue"),
        "收了的线头别再念：{after:?}"
    );
}

/// 「同模板 + 同锚点已经问过 ⇒ 不再生成」：新模板也走同一条链。
#[test]
fn an_asked_foreshadow_is_not_generated_twice() {
    let (_dir, mut store) = fresh();
    let threshold = UNCOLLECTED_CHAPTERS.max(1) as usize;
    let (work, chapters) = book_with_chapters(&mut store, threshold + 1);
    let id = planted_foreshadow(&mut store, work, "怀表", Some(chapters[0]));
    let anchor = format!("foreshadow:{id}");

    store.create_question_card(&card(work, "foreshadow.overdue", &[&anchor])).unwrap();
    let drafts = store.question_drafts(work).unwrap();
    assert!(
        !drafts.iter().any(|d| d.anchors.contains(&anchor)),
        "同一条伏笔不该再产草稿：{drafts:?}"
    );
}

/// 灵感那条线：**取引力最高的前几条**，锚点带两个（碎片自己 + 它挂的章节）。
#[test]
fn only_the_top_sparks_become_drafts_with_both_anchors() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded_book(&mut store);
    // 四条随手记的碎片，重要度拉开（同一天记下、都没用过，引力就由它说了算）
    let mut ids = Vec::new();
    for (body, importance) in
        [("最亮的一句", 0.9), ("次亮的一句", 0.7), ("还行的一句", 0.5), ("最暗的一句", 0.1)]
    {
        let id = jot(&mut store, work, body, vec![format!("chapter:{chapter}")]);
        store
            .conn()
            .execute("UPDATE fragments SET importance = ?1 WHERE id = ?2", (importance, id))
            .unwrap();
        ids.push(id);
    }

    let drafts = store.question_drafts(work).unwrap();
    let sparks: Vec<_> =
        drafts.iter().filter(|d| d.template_key == "spark.high_gravity").collect();
    assert_eq!(sparks.len(), SPARK_ELEMENTS, "取前 {SPARK_ELEMENTS} 条：{drafts:?}");
    for (draft, id) in sparks.iter().zip(&ids) {
        assert_eq!(
            draft.anchors,
            vec![format!("fragment:{id}"), format!("chapter:{chapter}")],
            "碎片锚点 + 它自己的章节锚点"
        );
        assert_eq!(draft.element, ElementKind::Spark);
    }
    assert!(
        !sparks.iter().any(|d| d.anchors.contains(&format!("fragment:{}", ids[3]))),
        "引力最低的那条不该进来：{drafts:?}"
    );
    // 重要度跟着引力走（同一天记下的，热度与新颖度一模一样）
    assert!(
        sparks[0].importance > sparks[1].importance && sparks[1].importance > sparks[2].importance,
        "重要度要跟着引力走：{:?}",
        sparks.iter().map(|d| d.importance).collect::<Vec<_>>()
    );
}

/// 挂在同一章上的第二条灵感**不会被第一条连坐**：去重认的是"这条碎片问过没有"。
///
/// （判据是"锚点全都在库里"而不是"任意一个在"——按"任意一个"算的话，
/// 第一章问过一次以后，所有挂在这一章上的灵感都会静默消失。）
#[test]
fn a_second_spark_on_the_same_chapter_still_gets_a_draft() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded_book(&mut store);
    let first = jot(&mut store, work, "第一句", vec![format!("chapter:{chapter}")]);
    let second = jot(&mut store, work, "第二句", vec![format!("chapter:{chapter}")]);

    // 第一条已经问过了（卡上带着它那两个锚点）
    store
        .create_question_card(&card(
            work,
            "spark.high_gravity",
            &[&format!("fragment:{first}"), &format!("chapter:{chapter}")],
        ))
        .unwrap();

    let drafts = store.question_drafts(work).unwrap();
    assert!(
        drafts.iter().any(|d| d.anchors.contains(&format!("fragment:{second}"))),
        "第二条碎片照样该问：{drafts:?}"
    );
    assert!(
        !drafts.iter().any(|d| d.anchors.contains(&format!("fragment:{first}"))),
        "问过的那条不该再产：{drafts:?}"
    );
}

/// 人物线（**派生的推进度**，不新建任何数据模型）：出场过、静够多章的人各产一条。
///
/// 判据一次摆开四档：刚好踩线（产）、差一章（不产）、最后一章里还出场（不产）、
/// 从没出场过（判不了 ⇒ 不产）。锚点是**人自己**（`entity:<id>`），不是某一章。
#[test]
fn a_person_who_has_been_quiet_for_long_enough_becomes_a_draft() {
    let threshold = STALLED_CAST_CHAPTERS.max(1) as usize;
    let (_dir, mut store) = fresh();
    // threshold + 1 章：下标 0 那一章出场的人 ⇒ 静了 threshold 章（刚好踩线）
    let (work, chapters) = book_with_chapters(&mut store, threshold + 1);
    let last_chapter = chapters[threshold];

    let stalled = person(&mut store, work, "陆文");
    store.set_node_cast(chapters[0], &[stalled], "test").unwrap();
    assert_eq!(
        quiet_chapters(&store, work, stalled),
        Some(threshold as i64),
        "静了多少章由存储层按正文单位算好"
    );

    // 差一章：下标 1 ⇒ 静了 threshold − 1 章
    let one_short = person(&mut store, work, "张三");
    store.set_node_cast(chapters[1], &[one_short], "test").unwrap();
    // 最后一章里还出场：静了 0 章（正热着）
    let still_hot = person(&mut store, work, "李四");
    store.set_node_cast(last_chapter, &[still_hot], "test").unwrap();
    // 从没在书里出场过：判不了"静了多少章"，绝不猜
    let never = person(&mut store, work, "王五");
    assert_eq!(quiet_chapters(&store, work, never), None, "没出场过就是判不了");

    let drafts = store.question_drafts(work).unwrap();
    let cast: Vec<_> = drafts.iter().filter(|d| d.template_key == "cast.stalled").collect();
    assert_eq!(cast.len(), 1, "只有静够久的那个人该问：{drafts:?}");
    assert_eq!(cast[0].anchors, vec![format!("entity:{stalled}")], "锚点是这个人自己");
    assert_eq!(cast[0].slots["cast"], "陆文");
    assert_eq!(cast[0].slots["chapters"], threshold.to_string());
    assert_eq!(cast[0].element, ElementKind::Cast);
    for (id, why) in [(one_short, "差一章"), (still_hot, "最后一章还在"), (never, "从没出场")] {
        assert!(
            !drafts.iter().any(|d| d.anchors.contains(&format!("entity:{id}"))),
            "{why}的人不该被问：{drafts:?}"
        );
    }
}

/// **同一个人出场好几章，也只产一条**（锚点就是他的身份），而且顺序稳、问过就不再来。
#[test]
fn a_person_appearing_in_many_chapters_still_gets_one_draft() {
    let threshold = STALLED_CAST_CHAPTERS.max(1) as usize;
    let (_dir, mut store) = fresh();
    // 出场三章（下标 0 / 1 / 2），末章下标 threshold + 2 ⇒ 最后一次出场后静了 threshold 章
    let (work, chapters) = book_with_chapters(&mut store, threshold + 3);
    let id = person(&mut store, work, "陆文");
    for node in &chapters[..3] {
        store.set_node_cast(*node, &[id], "test").unwrap();
    }

    let once = store.question_drafts(work).unwrap();
    let twice = store.question_drafts(work).unwrap();
    assert_eq!(once, twice, "同一本书两次生成结果必须一样");
    let cast: Vec<_> = once.iter().filter(|d| d.template_key == "cast.stalled").collect();
    assert_eq!(cast.len(), 1, "一个人只产一条：{once:?}");
    assert_eq!(cast[0].anchors, vec![format!("entity:{id}")]);
    assert_eq!(
        cast[0].slots["chapters"],
        (chapters.len() - 1 - 2).to_string(),
        "最后一次出场是下标 2 那一章（末章下标 {}）", chapters.len() - 1
    );

    // 问过（同模板 + 同锚点）就不再产——新模板走同一条去重链
    store
        .create_question_card(&card(work, "cast.stalled", &[&format!("entity:{id}")]))
        .unwrap();
    let after = store.question_drafts(work).unwrap();
    assert!(
        !after.iter().any(|d| d.template_key == "cast.stalled"),
        "同一个人不该再产草稿：{after:?}"
    );
}

/// **口径：人物静了多少章按"正文单位"算**——末章前夹着场景卡也不影响这个数
/// （与伏笔那条同一个坐标，见上面那条"两个判据必须同时成立"的测试）。
#[test]
fn scene_cards_do_not_change_how_long_a_person_has_been_quiet() {
    let threshold = STALLED_CAST_CHAPTERS.max(1) as usize;
    let (_dir, mut store) = fresh();
    // 差一章：章距 threshold − 1，本来就够不着阈值
    let (work, chapters) = book_with_chapters(&mut store, threshold);
    let id = person(&mut store, work, "陆文");
    store.set_node_cast(chapters[0], &[id], "test").unwrap();
    let asks = |store: &Store| {
        store
            .question_drafts(work)
            .unwrap()
            .into_iter()
            .any(|draft| draft.template_key == "cast.stalled")
    };

    assert_eq!(quiet_chapters(&store, work, id), Some(threshold as i64 - 1));
    assert!(!asks(&store), "差一章：还不该问");

    // 末章前挂两张场景卡（场景卡承载正文、会进叩问那份章节清单，但不是"章"）
    let last = chapters[chapters.len() - 1];
    for index in 0..2 {
        store.create_node(work, Some(last), NodeKind::Scene, &format!("场景{index}")).unwrap();
    }
    assert_eq!(
        quiet_chapters(&store, work, id),
        Some(threshold as i64 - 1),
        "夹着场景卡，静了多少章一个字都不该变"
    );
    assert!(!asks(&store), "场景卡不该把他顶过阈值");

    // 真加一章：这才踩线
    store.create_node(work, None, NodeKind::Chapter, "再多一章").unwrap();
    assert_eq!(quiet_chapters(&store, work, id), Some(threshold as i64));
    assert!(asks(&store), "真多一章才该问：{id}");
}

/// 名字空着的人物卡不产草稿：没有名字就问不出口（界面不替作者编名字）。
///
/// 建卡时名字不许空（`clean` 挡住了），所以这里直接把库里那一格抹掉，
/// 造出"外部改坏 / 老数据"那种行。
#[test]
fn a_nameless_person_gets_no_draft() {
    let threshold = STALLED_CAST_CHAPTERS.max(1) as usize;
    let (_dir, mut store) = fresh();
    let (work, chapters) = book_with_chapters(&mut store, threshold + 1);
    let id = person(&mut store, work, "陆文");
    store.set_node_cast(chapters[0], &[id], "test").unwrap();
    store
        .conn()
        .execute("UPDATE entity_cards SET name = '   ' WHERE id = ?1", [id])
        .unwrap();

    let drafts = store.question_drafts(work).unwrap();
    assert!(
        !drafts.iter().any(|d| d.template_key == "cast.stalled"),
        "名字空着的不问：{drafts:?}"
    );
}
