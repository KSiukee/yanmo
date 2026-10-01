//! 从**已写下的东西**里生成候选问题草稿：机制产问题。
//!
//! # 为什么是"草稿"而不是"卡"
//!
//! 草稿 = **模板键 + 槽位取值 + 关联锚点**，里面没有一个字的句子。句子由界面按语言渲染
//! （文案在界面字典里，核心零文案），作者点头之后再落成卡。这一层因此是纯函数：
//! 给一份「这本书此刻长什么样」，给回一串草稿——好测、可复现、可被命令行直接驱动。
//!
//! # 四路要素
//!
//! 章节树（有没有这一章、动笔没有）、每章一句话（计划要点）、字数（节奏），
//! 加上后来接进来的三路：伏笔那本账（埋了多久还没收）、碎片池的引力
//! （作者自己记下的句子，此刻最亮的几条）、人物线（出场过的人静了多久）。
//! **四路只是四份事实**：每接一路就是多一条规则，引力公式与排序一行都不用改。
//!
//! # 去重不在这里
//!
//! "同一个锚点上同一条模板的问题，已经问过就别再问"这件事要查库（已有哪些卡），
//! 所以它在存储层做（[`crate::store`]）；这里只管"按规则该问什么"。

use std::collections::BTreeMap;

use super::elements::{WritingElements, ChapterFacts, RhythmParams};
use super::rhythm;
use super::template::{template, ElementKind, QuestionTemplate};
use crate::model::ForeshadowState;
use crate::outline::UNCOLLECTED_CHAPTERS;

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

/// 模板池里那一条。走到这里键不在池子里说明 generate 与 `TEMPLATES` 走散了：
/// **开发期当场炸**，别静默少产一条问题。
fn spec_of(key: &'static str) -> &'static QuestionTemplate {
    template(key).unwrap_or_else(|| panic!("template {key} is missing from TEMPLATES"))
}

/// 照模板产一条草稿。
fn draft(key: &'static str, slots: &[(&str, String)], anchor: String) -> QuestionDraft {
    let spec = spec_of(key);
    QuestionDraft {
        template_key: spec.key,
        element: spec.element,
        slots: slots.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
        anchors: vec![anchor],
        importance: spec.importance,
    }
}

/// 槽位里的作者原文最多几个字（**按字符数**：中文按字算，别按字节切）。
///
/// 24 字：一句话的槽位取值要能跟模板句子一起读完——太长会把问题本身淹掉，
/// 太短又认不出是哪条伏笔 / 哪句灵感。截断只加省略号，**一个字都不改**。
const SLOT_TEXT_CHARS: usize = 24;

/// 槽位取值：抹平换行（作者原文可能是好几行）→ 截到 [`SLOT_TEXT_CHARS`] → 超出加省略号。
fn slot_text(raw: &str) -> String {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SLOT_TEXT_CHARS {
        return flat;
    }
    let mut out: String = flat.chars().take(SLOT_TEXT_CHARS).collect();
    // 省略号是**排版符号**，不是文案：任何语言下都长这样，不进字典
    out.push('…');
    out
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

    // ⑦ 埋了很久还没收的伏笔：这条线该收了（**阈值与大纲体检同一个来源**）
    out.extend(overdue_foreshadows(elements));

    // ⑧ 引力排到前头的那几条灵感碎片：你自己记下的句子，要不要用掉
    out.extend(high_gravity_sparks(elements));
    // ⑨ 静了很久的人物：这条线还接不接（**人物线**：从"谁出场过"这本账派生）
    out.extend(stalled_cast(elements));
    // ⑩ 静了很久的线：这条线要不要推一把（**五线**：读作者自己标过的 `node_lines`）
    out.extend(stalled_lines(elements));

    out
}

/// 每再拖一个阈值，超期 / 静默草稿的重要度涨这么多。
///
/// 取 0.2 的理由：伏笔模板的基准重要度是 0.8（见 `super::template::TEMPLATES`），
/// 加上 0.2 刚好封顶 1.0——也就是"拖到阈值的两倍"算拖到底。**踩线那一刻额外量是 0**：
/// 刚够格就问的那一条按基准排队，让位给拖得更久的那些（不最高，也不被埋掉）。
///
/// 人物线复用同一条曲线（它的基准是 0.6，所以要到**三倍**阈值才封顶）：
/// "静得越久越该接回来"与"埋得越久越该收"本来就是同一件事，没必要写第二个公式。
const OVERDUE_IMPORTANCE_STEP: f64 = 0.2;

/// 超期程度 → 这条草稿的重要度：`基准 + 超出的比例 × 步长`，封顶 1.0。
///
/// 为什么按**比例**而不是按绝对章数：阈值本身就是"多少章算久"，章节规模不同的书
/// （短篇 20 章 vs 长篇 200 章）用同一个比例才公平——`gap = 阈值` 时是 0，
/// `gap = 2 × 阈值` 时到顶。
fn overdue_importance(base: f64, gap: i64, threshold: i64) -> f64 {
    if threshold <= 0 {
        return base.clamp(0.0, 1.0);
    }
    let overdue = (gap - threshold).max(0) as f64 / threshold as f64;
    (base + overdue * OVERDUE_IMPORTANCE_STEP).clamp(0.0, 1.0)
}

/// 埋下、还没收、且隔了**大纲体检那个阈值**那么多章的伏笔各产一条草稿。
///
/// 四条分寸：
///
/// 1. **只看 `Planted`**：收了的不再问；"不写了"是正经结局（作者说过不走了，就别再念）——
///    与 `outline::rules` 的口径一致；
/// 2. **没记埋点的不产**（`chapters_since` 是 `None`）：判不了"隔了多少章"，绝不猜；
/// 3. **阈值与大纲体检是同一个数**（[`UNCOLLECTED_CHAPTERS`]）：体检说"该看看了"、
///    叩问问"往哪儿收"，是同一个判据的两张面孔，各写一个数迟早会分家；
/// 4. **一条伏笔只产一条**：锚点 `foreshadow:<id>` 就是它的身份（同一条伏笔的两个埋点
///    不存在，状态机只记一个埋点）。
///
/// 顺序＝ `elements.foreshadows` 的顺序（读库那一层给的稳定序），两次调用结果一样。
fn overdue_foreshadows(elements: &WritingElements) -> Vec<QuestionDraft> {
    let base = spec_of("foreshadow.overdue").importance;
    let mut out = Vec::new();
    for item in &elements.foreshadows {
        if item.state != ForeshadowState::Planted {
            continue;
        }
        // "隔了多少章"由存储层按**正文单位**那份清单算好（`text_spine`）——与大纲体检
        // 那条"埋了 20 章还没收"同一份坐标、同一个阈值；这里只比数，不再自己数一遍。
        let Some(gap) = item.chapters_since else { continue };
        if gap < UNCOLLECTED_CHAPTERS {
            continue;
        }
        let mut made = draft(
            "foreshadow.overdue",
            &[("foreshadow", slot_text(&item.body)), ("chapters", gap.to_string())],
            format!("foreshadow:{}", item.id),
        );
        // 提紧迫度：拖得越久越靠前（封顶 1.0，见 `overdue_importance`）
        made.importance = overdue_importance(base, gap, UNCOLLECTED_CHAPTERS);
        out.push(made);
    }
    out
}

/// 人物静了这么多章，这条线就该接回来了（阈值，旋钮）。
///
/// 取 12 的理由：一部长篇里，一个**出场过**的人整整十来回合没露面，读者先忘了他是谁——
/// 到那时再提"这条线接回来还是让他退场"才是提醒，早了是催（刚歇两章也要接回来，
/// 那叫多线并进，不叫断了）；12 就是**十来**回合那个数，与字典里温柔那版
/// 说的"十来回合以前"同一个口径，不另外编一个数。
///
/// 为什么不借伏笔那条 20：那个数说的是"埋了 20 章还没收"的**收账期限**（大纲体检
/// 与叩问共用的同一个判据），人物静一阵子是长篇的正常呼吸，两者不是一件事；
/// 共用一个数只会让改一个动两处。这个数**只有这一个消费者**（`stalled_cast`）。
pub const STALLED_CAST_CHAPTERS: i64 = 12;

/// 出场过、而且静了**足够多章**的人物各产一条草稿。
///
/// 四条分寸：
///
/// 1. **出场过才问**（`chapters_since` 是 `None` 就跳过）：从没在书里露过面的人不是
///    "这条线断了"，是"这条线还没起"——问"接回来还是让他退场"无从答起；
/// 2. **刚露面不问**（差一章也不问，判据是 `>=` 阈值）：最后一章里还出场的人正热着；
/// 3. **一个人只产一条**：锚点 `entity:<人物卡 id>` 就是他的身份；
/// 4. **静得越久越靠前**：重要度按超出的比例涨（封顶 1.0，与伏笔那条同一套写法，
///    见 `overdue_importance`——踩线那一刻还是模板基准）。
///
/// 顺序＝ `elements.characters` 的顺序（读库那层给的是按名字排的稳定序），两次调用结果一样。
fn stalled_cast(elements: &WritingElements) -> Vec<QuestionDraft> {
    let base = spec_of("cast.stalled").importance;
    let mut out = Vec::new();
    for person in &elements.characters {
        // "静了多少章"由存储层按**正文单位**那份清单算好（`text_spine`，见 `CharacterFacts`）；
        // 判不了的（从没出场 / 出场节点已不在那份清单里）跳过，绝不猜。
        let Some(gap) = person.chapters_since else { continue };
        if gap < STALLED_CAST_CHAPTERS {
            continue;
        }
        // 名字空着的跳过：**没有名字就问不出口**（存储层已经滤过一道，这里再兜一次——
        // 规则不该产出一条槽位是空的问题）。界面不替作者编名字。
        let name = slot_text(&person.name);
        if name.is_empty() {
            continue;
        }
        let mut made = draft(
            "cast.stalled",
            &[("cast", name), ("chapters", gap.to_string())],
            format!("entity:{}", person.id),
        );
        made.importance = overdue_importance(base, gap, STALLED_CAST_CHAPTERS);
        out.push(made);
    }
    out
}

/// 一条线连着这么多章没被推进，就该提一句（阈值，旋钮）。
///
/// 取 10 的理由：**线是最粗的一档**——一条线断了，读者跟着断的是这本书的主干；
/// 而"某个人物静了十来章"（[`STALLED_CAST_CHAPTERS`] = 12）是长篇里的正常呼吸。
/// 所以这个数要比人物的那一个**早一档**：一条线静过十来回合，已经不是呼吸，
/// 是一整段没往那儿走，该提了。为什么不是 8：八章常常就是短篇里的一整段偏，
/// 这时候问"要不要推一把"更像催；为什么不是 12：那等于与人物那条同档，
/// 而线比人粗，"粗的那一档先提"才说得通。
///
/// 为什么**不借** 12 或 20：`STALLED_CAST_CHAPTERS` 说的是"一个人物多久该接回来"、
/// `UNCOLLECTED_CHAPTERS` 说的是"一条伏笔多久该收"——三个数各有各的语义，
/// 共用一个数只会让改一个动三处。这个数**只有这一个消费者**（`stalled_lines`）。
pub const STALLED_LINE_CHAPTERS: i64 = 10;

/// 连着 [`STALLED_LINE_CHAPTERS`] 章没被推进、而且**作者标过**的线各产一条草稿。
///
/// 四条分寸：
///
/// 1. **标过才问**（`chapters_since` 是 `None` 就跳过）：**"没标"不等于"没推进"**
///    （`model::story_line` 写死的口径）——一次都没标过的线不是"断了"，是"还没说过"，
///    问"要不要推一把"无从答起；
/// 2. **刚推过不问**（差一章也不问，判据是 `>=` 阈值）：最后一章就在推这条线的，正热着；
/// 3. **一条线只产一条**：锚点 `line:<码>` 就是这条线的身份（一条线可能在好几章上标过，
///    取的只是"最后一次被推进"那一次）；
/// 4. **静得越久越靠前**：重要度按超出的比例涨（封顶 1.0，与伏笔、人物那两条同一套写法，
///    见 `overdue_importance`——踩线那一刻还是模板基准）。
///
/// ⚠️ 槽位 `line` 给的是**码**（`main` 这种），**不是中文名**：核心零文案，
/// 界面渲染那一层拿 `grid.lines.name.*` 那份字典翻成全名（与列上那五个徽标同一份键）。
///
/// 顺序＝ `elements.lines` 的顺序（读库那层给的是 [`crate::model::StoryLine::ALL`] 的固定序），
/// 两次调用结果一样。
fn stalled_lines(elements: &WritingElements) -> Vec<QuestionDraft> {
    let base = spec_of("line.stalled").importance;
    let mut out = Vec::new();
    for fact in &elements.lines {
        // "多久没被推过"由存储层按**正文单位**那份清单算好（`text_spine`，见 `LineFacts`）；
        // 判不了的（一次都没标过）跳过，绝不猜。
        let Some(gap) = fact.chapters_since else { continue };
        if gap < STALLED_LINE_CHAPTERS {
            continue;
        }
        let mut made = draft(
            "line.stalled",
            &[("line", fact.line.as_str().to_string()), ("chapters", gap.to_string())],
            format!("line:{}", fact.line.as_str()),
        );
        made.importance = overdue_importance(base, gap, STALLED_LINE_CHAPTERS);
        out.push(made);
    }
    out
}

/// 引力排到前头的那几条灵感碎片各产一条草稿。
///
/// 锚点给**两个**：`fragment:<碎片 id>`（这条碎片自己的身份）＋ 碎片自己的锚点原样并进去
/// （`chapter:12` 这种）——于是"跟着这一章走"（`store::question_select` 的按章优先）
/// 对灵感问题同样生效：作者正写着哪一章，挂在那一章上的灵感就先冒头。
///
/// 重要度＝**引力公式自己算出来的总分**（钳到 0~1）：不另立一套"叩问里的灵感权重"。
/// 引力大的先冒头，而分数的来源只有一个（`gravity::inspire`）。
fn high_gravity_sparks(elements: &WritingElements) -> Vec<QuestionDraft> {
    elements
        .sparks
        .iter()
        .map(|spark| {
            let mut made = draft(
                "spark.high_gravity",
                &[("spark", slot_text(&spark.body))],
                format!("fragment:{}", spark.fragment_id),
            );
            for anchor in &spark.anchors {
                // 碎片自己挂的锚点照抄；与碎片锚点重复时不写两遍（锚点是身份，不重复记）
                if !made.anchors.contains(anchor) {
                    made.anchors.push(anchor.clone());
                }
            }
            made.importance = spark.total.clamp(0.0, 1.0);
            made
        })
        .collect()
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
/// 比中位数长出一大截的那一章。
///
/// **判据不在本函数里**：中位数、样本门槛、倍数比对都在 [`super::rhythm`]——那里是
/// "特别长"的**唯一一处**定义，界面那条章长提醒读的也是它。这里只做"按阅读顺序挑出第一个"。
/// 章数太少就不评（两三章说不出"节奏"），由 [`super::rhythm::distribution`] 的闸门定。
fn very_long_chapter<'a>(
    chapters: &'a [ChapterFacts],
    p: &RhythmParams,
) -> Option<(&'a ChapterFacts, i64)> {
    let dist = rhythm::distribution(chapters);
    rhythm::samples(chapters)
        .into_iter()
        .find(|chapter| rhythm::exceeds(chapter.char_count, &dist, p.very_long_ratio))
        .map(|chapter| (chapter, chapter.char_count))
}
