//! 从**已写下的东西**里生成候选问题草稿（L1 规则版）：机制产问题。
//!
//! # 为什么是"草稿"而不是"卡"
//!
//! 草稿 = **模板键 + 槽位取值 + 关联锚点**，里面没有一个字的句子。句子由界面按语言渲染
//! （文案在界面字典里，核心零文案），作者点头之后再落成卡。这一层因此是纯函数：
//! 给一份「这本书此刻长什么样」，给回一串草稿——好测、可复现、可被命令行直接驱动。
//!
//! # L1 只用已有的数据
//!
//! 章节树（有没有这一章、动笔没有）、每章一句话（计划要点）、字数（节奏）。
//! 伏笔超期 / 五线推进度 / 灵感碎片引力这些要素是智能版的事——它们接进来时，
//! 只需多几条模板与规则，引力公式与排序一行都不用改。
//!
//! # 去重不在这里
//!
//! "同一个锚点上同一条模板的问题，已经问过就别再问"这件事要查库（已有哪些卡），
//! 所以它在存储层做（[`crate::store`]）；这里只管"按规则该问什么"。

use std::collections::BTreeMap;

use super::elements::{WritingElements, ChapterFacts, RhythmParams};
use super::template::{template, ElementKind, QuestionTemplate};

/// 一条候选问题草稿。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QuestionDraft {
    pub template_key: &'static str,
    pub element: ElementKind,
    /// 槽位名 → 取值（数字与作者数据；**不含任何界面文案**）
    pub slots: BTreeMap<String, String>,
    /// 关联锚点（如 `chapter:12`）——去重与"这条是拿哪一章问的"都靠它
    pub anchors: Vec<String>,
    pub importance: f64,
}

/// 照模板产一条草稿（模板键一定在池子里，故用 `expect` 而不是悄悄跳过）。
fn draft(key: &'static str, slots: &[(&str, String)], anchor: String) -> QuestionDraft {
    let spec: &'static QuestionTemplate =
        // 走到这里说明 generate 与 TEMPLATES 走散了：开发期当场炸，别静默少产一条问题
        template(key).unwrap_or_else(|| panic!("template {key} is missing from TEMPLATES"));
    QuestionDraft {
        template_key: spec.key,
        element: spec.element,
        slots: slots.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
        anchors: vec![anchor],
        importance: spec.importance,
    }
}

/// 照 L1 规则生成候选草稿（顺序稳定：章节顺序 → 要素顺序）。
pub fn generate(elements: &WritingElements, p: &RhythmParams) -> Vec<QuestionDraft> {
    let mut out = Vec::new();
    let chapters = &elements.chapters;

    // ① 第一个还没动笔的章：这一章从哪儿开始
    if let Some(c) = chapters.iter().find(|c| !c.has_body) {
        out.push(draft(
            "chapter.empty_body",
            &[("chapter", c.title.clone())],
            format!("chapter:{}", c.node_id),
        ));
    }

    // ② 写了正文、可一句话还空着的章：这一章发生了什么（完成度）
    for c in chapters.iter().filter(|c| c.has_body && c.summary.trim().is_empty()) {
        out.push(draft(
            "chapter.missing_summary",
            &[("chapter", c.title.clone())],
            format!("chapter:{}", c.node_id),
        ));
    }

    // ③ 连续几章字数雷同：读者可能会疲（节奏）
    if let Some((count, chars, last_id)) = same_length_run(chapters, p) {
        out.push(draft(
            "rhythm.length_swing",
            &[("count", count.to_string()), ("chars", chars.to_string())],
            format!("rhythm:swing:{last_id}"),
        ));
    }

    // ④ 明显比别人长的那一章：要不要拆（节奏）
    if let Some((c, chars)) = very_long_chapter(chapters, p) {
        out.push(draft(
            "rhythm.chapter_very_long",
            &[("chapter", c.title.clone()), ("chars", chars.to_string())],
            format!("rhythm:long:{}", c.node_id),
        ));
    }

    // ⑤ 最近写完的那一章：接下来接着哪条线走（承上启下）
    if let Some(c) = chapters.iter().rev().find(|c| c.has_body) {
        out.push(draft(
            "review.recent_chapter",
            &[("chapter", c.title.clone())],
            format!("chapter:{}", c.node_id),
        ));
    }

    // ⑥ 开篇还没落笔（阅读顺序里第一章还空着）：问"谁在看"与"第一场戏在哪儿"。
    //
    // 为什么挑第一个承载正文的节点：它才是这本书的开头——卷不承载正文，不会出现在这里
    // （`writing_elements` 已经把容器滤掉了）。一旦开头写下了第一句，这两问就不再出现：
    // 视角与第一场戏是**落笔之前**要定的事，写起来了再问就成了马后炮。
    if let Some(first) = chapters.first().filter(|c| !c.has_body) {
        for key in ["plan.opening_pov", "plan.opening_scene"] {
            out.push(draft(
                key,
                &[("chapter", first.title.clone())],
                format!("chapter:{}", first.node_id),
            ));
        }
    }

    out
}

/// 最近 `swing_window` 章**都写了**、且字数极差在均值的一小截之内 → 返回（章数、均值、末章 id）。
fn same_length_run(chapters: &[ChapterFacts], p: &RhythmParams) -> Option<(usize, i64, i64)> {
    if p.swing_window < 2 {
        return None;
    }
    let written: Vec<&ChapterFacts> = chapters.iter().filter(|c| c.has_body).collect();
    if written.len() < p.swing_window {
        return None;
    }
    let window = &written[written.len() - p.swing_window..];
    let counts: Vec<i64> = window.iter().map(|c| c.char_count).collect();
    let min = *counts.iter().min()?;
    let max = *counts.iter().max()?;
    let sum: i64 = counts.iter().sum();
    let avg = sum / counts.len() as i64;
    if avg <= 0 {
        return None;
    }
    let spread = (max - min) as f64 / avg as f64;
    if spread > p.swing_spread {
        return None;
    }
    Some((counts.len(), avg, window.last()?.node_id))
}

/// 比中位数长出一大截的那一章（章数太少就不评——两三章说不出"节奏"）。
fn very_long_chapter<'a>(
    chapters: &'a [ChapterFacts],
    p: &RhythmParams,
) -> Option<(&'a ChapterFacts, i64)> {
    let written: Vec<&ChapterFacts> = chapters.iter().filter(|c| c.has_body && c.char_count > 0).collect();
    if written.len() < 3 {
        return None;
    }
    let mut counts: Vec<i64> = written.iter().map(|c| c.char_count).collect();
    counts.sort_unstable();
    let median = counts[counts.len() / 2];
    if median <= 0 {
        return None;
    }
    let limit = (median as f64 * p.very_long_ratio) as i64;
    written
        .into_iter()
        .find(|c| c.char_count > limit)
        .map(|c| (c, c.char_count))
}
