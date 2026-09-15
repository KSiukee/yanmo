//! 叩问「处置四件套」验收：**舍弃进冷却库（可捞回 + 当负样本）／按来源静音／记灵感派生与溯源**，
//! 外加一条由"记灵感"引出的**真 bug 回归**：派生链深必须能穿过灵感卡。
//!
//! 判据都落在可核对的事实上：冷却库读得回来、捞回真的回池子、静音来源整批不出现、
//! 记灵感**不动问题状态**、灵感能溯源回问题、问题→灵感→问题 的链深照样卡得住。

use yanmo_core::model::{NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::question::DeferCondition;
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn card(work_id: i64, source: &str) -> NewQuestionCard {
    NewQuestionCard {
        work_id,
        body: "这条问题先放一放".to_string(),
        source: source.to_string(),
        template_key: "chapter.empty_body".to_string(),
        importance: 0.5,
        linked: Vec::new(),
        derived_from: None,
        auto_derived: false,
    }
}

fn seeded(store: &mut Store) -> i64 {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    work.id
}

#[test]
fn discarding_sends_the_card_to_the_cooled_shelf_and_it_can_come_back() {
    let (_dir, mut store) = fresh();
    let work = seeded(&mut store);
    let kept = store.create_question_card(&card(work, "core")).unwrap();
    let dropped = store.create_question_card(&card(work, "core")).unwrap();
    store.move_question_card(dropped, QuestionState::Discarded, "author").unwrap();

    assert!(
        store.select_questions(work, 10).unwrap().iter().all(|q| q.card_id == kept),
        "舍弃的卡不再出现在候选里"
    );
    let cooled = store.cooled_questions(work).unwrap();
    assert_eq!(cooled.len(), 1);
    assert_eq!(cooled[0].card_id, dropped);
    assert_eq!(cooled[0].body, "这条问题先放一放");
    assert!(cooled[0].cooled_at > 0, "冷却库里要看得出什么时候放下的");

    // 负样本当场就学到了：同类模板降权
    let weight = store.template_weight("chapter.empty_body").unwrap();
    assert!(weight.weight < 1.0 && weight.negatives == 1, "{weight:?}");

    // 捞回：回到候选池，冷却库里不再有它
    store.move_question_card(dropped, QuestionState::Pending, "author").unwrap();
    assert!(store.cooled_questions(work).unwrap().is_empty());
    assert!(store.select_questions(work, 10).unwrap().iter().any(|q| q.card_id == dropped));
}

#[test]
fn muting_a_source_puts_that_module_away_without_turning_questioning_off() {
    let (_dir, mut store) = fresh();
    let work = seeded(&mut store);
    let ours = store.create_question_card(&card(work, "core")).unwrap();
    let noisy = store.create_question_card(&card(work, "module-x")).unwrap();

    store.mute_source("module-x").unwrap();
    assert_eq!(store.muted_sources().unwrap(), vec!["module-x".to_string()]);
    let picked = store.select_questions(work, 10).unwrap();
    assert!(picked.iter().all(|q| q.card_id != noisy), "静音来源的问题不出现");
    assert!(picked.iter().any(|q| q.card_id == ours), "别的来源照常");

    // 解除之后又回来
    store.unmute_source("module-x").unwrap();
    assert!(store.muted_sources().unwrap().is_empty());
    assert!(store.select_questions(work, 10).unwrap().iter().any(|q| q.card_id == noisy));

    // 重复静音不叠行；坏记录当没静音（不炸、也不静默改语义）
    store.mute_source("module-x").unwrap();
    store.mute_source("module-x").unwrap();
    assert_eq!(store.muted_sources().unwrap().len(), 1);
    store
        .conn()
        .execute(
            "UPDATE settings SET value = 'not json' WHERE key = 'question.muted_sources'",
            [],
        )
        .unwrap();
    assert!(store.muted_sources().unwrap().is_empty(), "读不动的偏好当没设过");
}

#[test]
fn recording_an_inspiration_never_touches_the_question() {
    let (_dir, mut store) = fresh();
    let work = seeded(&mut store);
    let id = store.create_question_card(&card(work, "core")).unwrap();
    store.move_question_card(id, QuestionState::Asked, "push").unwrap();

    let idea_id = store
        .record_question_inspiration(id, "  让他把那封信烧了  ", "typed", "author")
        .unwrap();
    let idea = store.inspiration(idea_id).unwrap();
    assert_eq!(idea.body, "让他把那封信烧了", "首尾空白不算");
    assert_eq!(idea.derived_from, Some(id), "溯源指回问题卡");
    assert_eq!(idea.source, "typed");
    assert_eq!(idea.work_id, work);

    // **正交**：问题的状态与新颖度的账一个字节都没动
    let after = store.question_card(id).unwrap();
    assert_eq!(after.state, QuestionState::Asked);
    assert_eq!(after.used_count, 1, "记灵感不算问出");

    // 溯源查得回来；事件里也留着
    let ideas = store.inspirations_of_question(id).unwrap();
    assert_eq!(ideas.len(), 1);
    assert_eq!(ideas[0].id, idea_id);
    // 事件挂在**灵感卡**上（它才是这次写入的实体），payload 里带着它是从哪张卡派生的
    assert!(store.card_events(idea_id).unwrap().iter().any(|e| e.op == "inspire"));

    // 空内容与不存在的卡都要当场说清
    assert_eq!(
        store.record_question_inspiration(id, "   ", "typed", "author").unwrap_err().code(),
        "idea.body_empty"
    );
    assert_eq!(store.inspiration(9999).unwrap_err().code(), "idea.not_found");
}

/// 两个**回头路**：静音的类别能解除、延后能取消。
///
/// 少了它们，界面里就出现两个只进不出的开关——这个项目最烦的就是"出不去"。
#[test]
fn both_new_switches_have_a_way_back() {
    let (_dir, mut store) = fresh();
    let work = seeded(&mut store);

    // ① 「这类别再问」→ 类别静音；解除之后同类的问题又回候选池
    let muted = store.create_question_card(&card(work, "core")).unwrap();
    store.move_question_card(muted, QuestionState::Muted, "author").unwrap();
    assert_eq!(store.muted_templates().unwrap(), vec!["chapter.empty_body".to_string()]);
    let back = store.create_question_card(&card(work, "core")).unwrap();
    assert!(
        store.select_questions(work, 10).unwrap().is_empty(),
        "这一类被静音了，新卡也不该出现"
    );
    store.unmute_template("chapter.empty_body").unwrap();
    assert!(store.muted_templates().unwrap().is_empty(), "解除之后名单里没有它");
    assert!(
        store.select_questions(work, 10).unwrap().iter().any(|q| q.card_id == back),
        "解除之后这一类的问题又回来了"
    );

    // ② 「我自己想起来再问」→ 延后；取消之后当场回候选池，记录也标掉了
    store.move_question_card(back, QuestionState::Asked, "push").unwrap();
    store
        .defer_question_card(back, DeferCondition::manual(), "回头再说", "author")
        .unwrap();
    assert!(store.select_questions(work, 10).unwrap().is_empty(), "延后期间不出现");
    store.cancel_deferral(back, "author").unwrap();
    assert_eq!(store.question_card(back).unwrap().state, QuestionState::Pending);
    assert!(store.open_deferrals(work).unwrap().is_empty(), "那条记录要标掉，不能再挂着");
    assert_eq!(store.card_deferrals(back).unwrap()[0].note, "回头再说", "作者那句话留着");
    assert!(store.select_questions(work, 10).unwrap().iter().any(|q| q.card_id == back));

    // 不在延后态的卡不能"取消延后"（状态机当场拒）
    let err = store.cancel_deferral(back, "author").unwrap_err();
    assert_eq!(err.code(), "card.illegal_transition");
}

/// 真 bug 回归（2026-09-15 落地"记灵感"时当场发现）：
/// 派生链深只沿"问题卡"这一类的父级走的话，**问题 → 灵感 → 问题** 这条链会断在灵感卡上，
/// 防自激的深度限制就形同虚设。现在必须穿过灵感卡照样数得出来。
#[test]
fn the_derivation_chain_counts_through_inspiration_cards() {
    let (_dir, mut store) = fresh();
    let work = seeded(&mut store);
    let root = store.create_question_card(&card(work, "core")).unwrap();

    // 「问题 → 灵感 → 由该灵感自动生成的问题」＝两跳，**到此为止**（设计口径）
    let idea = store
        .record_question_inspiration(root, "也许该让他留在城里", "typed", "author")
        .unwrap();
    store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(idea),
            auto_derived: true,
            ..card(work, "core")
        })
        .expect("第一跳自动派生：可以（链深 2 层内）");

    // 唯一能拦住"无限套娃"的地方，就是**链穿过灵感卡**时照样数得清：
    // 要是只沿"问题卡"这一类走，这里会以为链断了、放它过去。
    let hop1 = store.inspirations_of_question(root).unwrap()[0].id;
    let deeper = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(hop1),
            auto_derived: true,
            ..card(work, "core")
        })
        .unwrap();
    let idea2 = store.record_question_inspiration(deeper, "那就让他先走一趟", "typed", "author").unwrap();
    let err = store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(idea2),
            auto_derived: true,
            ..card(work, "core")
        })
        .unwrap_err();
    assert_eq!(err.code(), "card.derivation_too_deep", "链深到顶，自动派生不再往下");

    // 作者手动顺着灵感再问，不受此限
    store
        .create_question_card(&NewQuestionCard {
            derived_from: Some(idea2),
            auto_derived: false,
            ..card(work, "core")
        })
        .expect("作者手动再问不受链深限制");
}
