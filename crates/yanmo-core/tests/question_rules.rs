//! 生成规则的验收（**纯逻辑**，不需要库）：一条规则什么时候该产问题、产出来的槽位填没填满。
//!
//! 与另外两份测试的分工：`gravity_vectors` 盯机制对齐、这一份盯"照书里的东西该问什么"、
//! `question_engine` 盯"落库之后的选择与学习"。

use yanmo_core::question::{
    generate, template, ChapterFacts, QuestionDraft, RhythmParams, WritingElements,
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

fn keys(drafts: &[QuestionDraft]) -> Vec<&'static str> {
    drafts.iter().map(|d| d.template_key).collect()
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
    };
    for d in generate(&elements, &RhythmParams::default()) {
        let spec = template(d.template_key).expect("草稿的模板一定要在池子里");
        for slot in spec.slots {
            assert!(d.slots.contains_key(*slot), "{} 缺槽位 {slot}", d.template_key);
        }
    }
}
