//! 生成规则的验收（**纯逻辑**，不需要库）：一条规则什么时候该产问题、产出来的槽位填没填满。
//!
//! 与另外两份测试的分工：`gravity_vectors` 盯机制对齐、这一份盯"照书里的东西该问什么"、
//! `question_engine` 盯"落库之后的选择与学习"。

use yanmo_core::model::ForeshadowState;
use yanmo_core::outline::UNCOLLECTED_CHAPTERS;
use yanmo_core::question::{
    generate, template, ChapterFacts, ForeshadowFacts, QuestionDraft, RhythmParams, SparkFacts,
    WritingElements,
};

fn chapter(id: i64, title: &str, chars: i64, summary: &str) -> ChapterFacts {
    ChapterFacts {
        node_id: id,
        title: title.to_string(),
        has_body: chars > 0,
        char_count: chars,
        summary: summary.to_string(),
    }
}

/// 一条伏笔的事实（**"隔了多少章"直接给**——这个数由存储层按"正文单位"那份清单算好，
/// 规则只比数；见 `ForeshadowFacts` 的说明）。
fn foreshadow(id: i64, body: &str, state: ForeshadowState, chapters_since: Option<i64>) -> ForeshadowFacts {
    ForeshadowFacts { id, body: body.to_string(), state, chapters_since }
}

/// `n` 章**都写了字**的清单（伏笔那条规则要一摞章来数"隔了多少章"）。
fn chapters(n: usize) -> Vec<ChapterFacts> {
    (0..n).map(|at| chapter(at as i64 + 1, "某一章", 1000, "一句话")).collect()
}

fn keys(drafts: &[QuestionDraft]) -> Vec<&'static str> {
    drafts.iter().map(|d| d.template_key).collect()
}

/// 只留下某条模板的草稿（一本有多种要素时好挑）。
fn only(drafts: Vec<QuestionDraft>, key: &str) -> Vec<QuestionDraft> {
    drafts.into_iter().filter(|draft| draft.template_key == key).collect()
}

#[test]
fn empty_book_asks_nothing() {
    let drafts = generate(&WritingElements::default(), &RhythmParams::default());
    assert!(drafts.is_empty(), "什么都没有的时候不该硬造问题：{drafts:?}");
}

#[test]
fn new_book_asks_where_to_start() {
    let elements = WritingElements {
        chapters: vec![
            chapter(1, "第一章", 0, ""),
            chapter(2, "第二章", 0, ""),
        ],
        ..Default::default()
    };
    let drafts = generate(&elements, &RhythmParams::default());
    assert_eq!(
        keys(&drafts),
        vec!["chapter.empty_body", "plan.opening_pov", "plan.opening_scene"],
        "开头还空着：除了「从哪儿开始」，还要问「谁在看」与「第一场戏在哪儿」"
    );
    assert_eq!(drafts[0].slots["chapter"], "第一章");
    assert_eq!(drafts[0].anchors, vec!["chapter:1"]);
    // 开篇两问都锚在**开头那一章**上（不是第二、第三章）
    for draft in &drafts[1..] {
        assert_eq!(draft.slots["chapter"], "第一章");
        assert_eq!(draft.anchors, vec!["chapter:1"]);
    }
}

/// 开头一旦落笔，开篇两问就不再出现——视角与第一场戏是**落笔前**要定的事。
#[test]
fn opening_questions_stop_once_the_first_chapter_is_written() {
    let elements = WritingElements {
        chapters: vec![
            chapter(1, "第一章", 1800, ""),
            chapter(2, "第二章", 0, ""),
        ],
        ..Default::default()
    };
    let drafts = generate(&elements, &RhythmParams::default());
    assert!(
        !keys(&drafts).iter().any(|key| key.starts_with("plan.opening")),
        "开头写过了就别再问开篇的事：{:?}",
        keys(&drafts)
    );
}

#[test]
fn written_chapter_without_a_line_and_the_latest_chapter_are_asked() {
    let elements = WritingElements {
        chapters: vec![
            chapter(1, "第一章", 2000, "他回了家。"),
            chapter(2, "第二章", 2100, ""),
        ],
        ..Default::default()
    };
    let drafts = generate(&elements, &RhythmParams::default());
    assert_eq!(
        keys(&drafts),
        vec!["chapter.missing_summary", "review.recent_chapter"],
        "没留一句话的章要问，最近写完的章也要问"
    );
    assert_eq!(drafts[1].slots["chapter"], "第二章");
}

#[test]
fn rhythm_rules_fire_on_flat_and_on_oversized_chapters() {
    let flat = WritingElements {
        chapters: vec![
            chapter(1, "第一章", 1000, "甲"),
            chapter(2, "第二章", 1020, "乙"),
            chapter(3, "第三章", 1010, "丙"),
        ],
        ..Default::default()
    };
    assert!(
        keys(&generate(&flat, &RhythmParams::default())).contains(&"rhythm.length_swing"),
        "三章字数几乎一样，该问节奏"
    );

    let spiky = WritingElements {
        chapters: vec![
            chapter(1, "第一章", 1000, "甲"),
            chapter(2, "第二章", 1100, "乙"),
            chapter(3, "第三章", 9000, ""),
        ],
        ..Default::default()
    };
    let drafts = generate(&spiky, &RhythmParams::default());
    assert!(keys(&drafts).contains(&"rhythm.chapter_very_long"), "{drafts:?}");
}

/// 每个槽位都真的填上了值：界面渲染时不会剩下一个 `{chapter}`。
#[test]
fn every_slot_is_filled() {
    let elements = WritingElements {
        chapters: vec![
            chapter(1, "第一章", 1000, "甲"),
            chapter(2, "第二章", 1020, "乙"),
            chapter(3, "第三章", 1010, ""),
        ],
        ..Default::default()
    };
    for d in generate(&elements, &RhythmParams::default()) {
        let spec = template(d.template_key).expect("草稿的模板一定要在池子里");
        for slot in spec.slots {
            assert!(d.slots.contains_key(*slot), "{} 缺槽位 {slot}", d.template_key);
        }
    }
}

/// 伏笔超期：**刚好到阈值**（与大纲体检同一个数）产一条，差一章不产。
///
/// 阈值取自 `outline::UNCOLLECTED_CHAPTERS`——两处口径必须同一个来源，
/// 所以这里也不写死 20：写死的话，改了阈值这测试照样绿（那就白测了）。
#[test]
fn a_foreshadow_is_asked_only_when_it_is_overdue_by_the_shared_threshold() {
    let threshold = UNCOLLECTED_CHAPTERS.max(1) as usize;
    // 隔了 threshold 章（刚好踩线）——这个数由存储层算好，这里直接给
    let at_threshold = WritingElements {
        chapters: chapters(threshold + 1),
        foreshadows: vec![foreshadow(1, "老张的怀表", ForeshadowState::Planted, Some(threshold as i64))],
        ..Default::default()
    };
    let drafts = only(generate(&at_threshold, &RhythmParams::default()), "foreshadow.overdue");
    assert_eq!(drafts.len(), 1, "刚好到阈值就该问：{drafts:?}");
    assert_eq!(drafts[0].anchors, vec!["foreshadow:1"], "锚点是这一条伏笔自己");
    assert_eq!(drafts[0].slots["chapters"], threshold.to_string());
    assert_eq!(drafts[0].slots["foreshadow"], "老张的怀表");

    // 少一章（threshold - 1）→ 不问
    let one_short = WritingElements {
        chapters: chapters(threshold + 1),
        foreshadows: vec![
            foreshadow(1, "老张的怀表", ForeshadowState::Planted, Some(threshold as i64 - 1)),
        ],
        ..Default::default()
    };
    assert!(
        only(generate(&one_short, &RhythmParams::default()), "foreshadow.overdue").is_empty(),
        "差一章都不该问"
    );

    // 一章都没有：没有"隔了多少章"这回事
    assert!(generate(&WritingElements::default(), &RhythmParams::default()).is_empty());
}

/// 收了的不问、"不写了"不问、没记埋点的不问。
#[test]
fn collected_dropped_and_unplanted_foreshadows_stay_silent() {
    let book = |state: ForeshadowState, since: Option<i64>| WritingElements {
        chapters: chapters(UNCOLLECTED_CHAPTERS.max(1) as usize + 5),
        foreshadows: vec![foreshadow(7, "那封信", state, since)],
        ..Default::default()
    };
    let overdue = UNCOLLECTED_CHAPTERS * 3; // 明显超期：唯一变量只剩状态
    for (state, since, why) in [
        (ForeshadowState::Collected, Some(overdue), "收了的线头别再念"),
        (ForeshadowState::Dropped, Some(overdue), "「不写了」是正经结局"),
        (ForeshadowState::Planted, None, "没记埋点就判不了，绝不猜"),
    ] {
        let drafts = only(generate(&book(state, since), &RhythmParams::default()), "foreshadow.overdue");
        assert!(drafts.is_empty(), "{why}：{drafts:?}");
    }
}

/// 提紧迫度：**拖得越久重要度越高**，封顶 1.0，而且踩线那一刻不是最高。
#[test]
fn the_longer_a_foreshadow_is_overdue_the_more_it_matters() {
    let threshold = UNCOLLECTED_CHAPTERS.max(1) as usize;
    let importance_at = |gap: usize| {
        let elements = WritingElements {
            chapters: chapters(threshold * 3),
            foreshadows: vec![
                foreshadow(1, "怀表", ForeshadowState::Planted, Some(gap as i64)),
            ],
            ..Default::default()
        };
        only(generate(&elements, &RhythmParams::default()), "foreshadow.overdue")
            .first()
            .expect("足够久，该产一条")
            .importance
    };

    let base_of_template = template("foreshadow.overdue").unwrap().importance;
    let barely = importance_at(threshold); // 刚好踩线
    let halfway = importance_at(threshold * 3 / 2); // 拖到 1.5 倍
    let capped = importance_at(threshold * 2); // 拖到 2 倍
    let beyond = importance_at(threshold * 3 - 1); // 拖到底

    assert!(
        (barely - base_of_template).abs() < 1e-12 && barely < 1.0,
        "刚够格就是模板基准，不该一上来就最高：{barely}"
    );
    assert!(halfway > barely, "拖得久的要更靠前：{halfway} vs {barely}");
    assert!(halfway < 1.0, "还没拖到底，不该已经封顶：{halfway}");
    assert!((capped - 1.0).abs() < 1e-12, "两倍阈值就该到顶：{capped}");
    assert!((beyond - 1.0).abs() < 1e-12, "再拖也不许超过 1.0：{beyond}");
}

/// 灵感碎片：每条产一条，锚点**两个**（碎片自己 + 它挂的章节），重要度来自引力总分。
#[test]
fn each_spark_becomes_a_draft_anchored_to_itself_and_its_chapter() {
    let elements = WritingElements {
        chapters: chapters(2),
        sparks: vec![
            SparkFacts {
                fragment_id: 11,
                body: "她把信折成一只鸟".to_string(),
                total: 0.42,
                anchors: vec!["chapter:2".to_string()],
            },
            SparkFacts {
                fragment_id: 12,
                body: "没有章节的碎句".to_string(),
                total: 0.18,
                anchors: Vec::new(),
            },
        ],
        ..Default::default()
    };
    let drafts = only(generate(&elements, &RhythmParams::default()), "spark.high_gravity");
    assert_eq!(drafts.len(), 2, "取来的每条碎片各产一条：{drafts:?}");

    let first = &drafts[0];
    assert_eq!(first.anchors, vec!["fragment:11", "chapter:2"], "碎片锚点 + 它自己的章节锚点");
    assert_eq!(first.slots["spark"], "她把信折成一只鸟");
    assert!((first.importance - 0.42).abs() < 1e-12, "重要度就是引力总分");
    // 没挂锚点的碎片照样产，只是只有自己那一个锚点
    assert_eq!(drafts[1].anchors, vec!["fragment:12"]);
    assert!(drafts[0].importance > drafts[1].importance, "引力大的更重要");
}

/// 槽位里的作者原文：太长截到 24 字加省略号，换行抹平；短的照原样——**一个字都不改**。
#[test]
fn long_bodies_are_clipped_with_an_ellipsis() {
    let threshold = UNCOLLECTED_CHAPTERS.max(1) as usize;
    let long = "一".repeat(40);
    let elements = WritingElements {
        chapters: chapters(threshold + 1),
        sparks: vec![SparkFacts {
            fragment_id: 1,
            body: format!("第一行\n第二行{long}"),
            total: 0.5,
            anchors: Vec::new(),
        }],
        // 隔了 threshold 章：刚好该问（这条测的是截断，不是阈值）
        foreshadows: vec![
            foreshadow(2, "短句", ForeshadowState::Planted, Some(UNCOLLECTED_CHAPTERS)),
        ],
        ..Default::default()
    };
    let drafts = generate(&elements, &RhythmParams::default());

    let spark = only(drafts.clone(), "spark.high_gravity").remove(0);
    let text = &spark.slots["spark"];
    assert!(text.ends_with('…'), "截断要加省略号：{text}");
    assert_eq!(text.chars().count(), 25, "24 字 + 一个省略号：{text}");
    assert!(!text.contains('\n'), "换行要抹平，不然问题句子会被拆断：{text:?}");
    assert!(text.starts_with("第一行 第二行"), "抹平换行是把它换成空格：{text:?}");

    let over = only(drafts, "foreshadow.overdue").remove(0);
    assert_eq!(over.slots["foreshadow"], "短句", "不到 24 字一个字都不动");
    assert!(!over.slots["foreshadow"].contains('…'));
}

/// 顺序稳定：同一份要素跑两遍，结果一模一样（含伏笔与灵感的相对顺序）。
#[test]
fn the_same_elements_always_give_the_same_drafts() {
    let elements = WritingElements {
        chapters: chapters(UNCOLLECTED_CHAPTERS.max(1) as usize + 1),
        foreshadows: vec![
            foreshadow(1, "甲", ForeshadowState::Planted, Some(UNCOLLECTED_CHAPTERS)),
            foreshadow(2, "乙", ForeshadowState::Planted, Some(UNCOLLECTED_CHAPTERS)),
        ],
        sparks: vec![SparkFacts {
            fragment_id: 9,
            body: "丙".to_string(),
            total: 0.3,
            anchors: vec!["chapter:1".to_string()],
        }],
        ..Default::default()
    };
    let once = generate(&elements, &RhythmParams::default());
    let twice = generate(&elements, &RhythmParams::default());
    assert_eq!(once, twice);
    // 同一条伏笔只产一条
    let overdue = only(once, "foreshadow.overdue");
    assert_eq!(overdue.len(), 2);
    assert_eq!(overdue[0].anchors, vec!["foreshadow:1"]);
    assert_eq!(overdue[1].anchors, vec!["foreshadow:2"]);
}
