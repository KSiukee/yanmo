//! 从正文里认**名字**：把人 / 地名抽出来，再跟设定卡比对，报出"正文里有、设定里没有"的。
//!
//! 这是 `outline::rules` 里**唯一**要读正文的一条——其余五条全靠结构化数据。
//! 判据在 [`crate::outline::mention_words`] 的四张表上；这一层只管"怎么用它们"。
//!
//! # 只做判得动的（假警报比不报更糟）
//!
//! 中文没有分词就没有"名字"这个概念，认名字只能靠**硬信号**。这里只用两种：
//!
//! | 帧 | 判据 | 例子 |
//! |---|---|---|
//! | **称谓** | 正文里出现一个称谓词，它左边还有名字字 | 李大人 / 王公子 / 赵姑娘 |
//! | **姓名** | 常用单字姓 + 一两个字，**紧跟着说话或动作词** | 张三说 / 李四点了点头 / 王小明笑道 |
//!
//! 地名走**通名**那一类：左边有名字字的通名（山/城/宗/门…）→ 青城山 / 天剑宗。
//!
//! # 两条保命的规矩
//!
//! 1. **起点认不出就不要**：名字的左边必须找得到**断点**（标点/空白/正文开头，或者一个
//!    虚词、代词、量词、常见动作动词）。往前找三个字还没碰上断点，说明"名字"是从某个词
//!    中间切出来的（"走出**青城**山"能切出"出青城山"这种东西）——那一条宁可不要。
//!    所以「长安城」认得出来、「上海」认不出来（"上"是断点）：**宁漏不猜**。
//! 2. **门槛与上限**：人名整本书出现 2 次以上、地名 3 次以上才算候选（出现一次的多半是
//!    巧合）；一屏最多 [`UNLISTED_MAX`] 条，**截掉几条如实报**（不静默丢）。
//!
//! # 报出来的是**候选**，不是错
//!
//! 它与其余五条规则不同：那五条说的是"这里对不上"，这一条说的是"这个名字我认不出来，
//! 你看看是不是漏建了卡"。所以界面上给的是"建成人物卡 / 设定卡"与"这不是名字"两个出口，
//! 而不是"去改"。

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{EntityCard, EntityKind};

use super::issue::{IssueRule, OutlineIssue};
use super::mention_words::{
    ACTION_MARKERS, BAD_LONE_PREFIX, BOUNDARY, NUMERALS, PERSON_TITLES, PLACE_SUFFIXES, SURNAMES,
};

/// 名字的起点往前最多找几个字（再多就该碰上一个断点了，碰不上说明起点不可信）。
const PREFIX_MAX: usize = 3;
/// 人名整本书出现几次才算候选。
const PERSON_MIN: usize = 2;
/// 地名整本书出现几次才算候选（比人名严一档：地名靠单字通名，噪声本来就更大）。
const PLACE_MIN: usize = 3;
/// 一屏最多报几条（按出现次数从多到少留；超出的**如实报数**，不静默丢）。
pub const UNLISTED_MAX: usize = 40;

/// 一个"正文里有、设定卡上没有"的候选。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UnlistedName {
    /// 正文里的写法（原样，不是规范化后的）
    pub form: String,
    /// 猜它是什么（建卡时当默认那一页）
    pub kind: EntityKind,
    /// 整本书里出现过几次
    pub count: usize,
    /// 第一次出现在哪个节点（界面拿它跳过去；**不进身份**，见 `to_issue`）
    pub first_node: i64,
}

/// 抽出全部候选（**稳定序**：出现次数从多到少，再按类别与写法）。
///
/// `chapters` 按阅读顺序给 `(节点 id, 正文)`；`cards` 是这本书的全部设定卡。
pub fn unlisted_names(chapters: &[(i64, &str)], cards: &[EntityCard]) -> Vec<UnlistedName> {
    let known = known_tokens(cards);
    // 写法 → (类别, 出现过的位置)。位置用集合：同一处被两种帧都认到时只算一次。
    let mut hits: BTreeMap<&str, (EntityKind, BTreeSet<(usize, usize)>)> = BTreeMap::new();
    for (index, entry) in chapters.iter().enumerate() {
        let (_, body) = *entry;
        collect_titles(body, index, &known, &mut hits);
        collect_places(body, index, &known, &mut hits);
        collect_surnames(body, index, &known, &mut hits);
    }

    let mut out: Vec<UnlistedName> = hits
        .into_iter()
        .filter_map(|(form, (kind, spots))| {
            // 门槛按类别不同（地名更严）：不够格的不进候选，连"我认不出"都不说
            let floor = if kind == EntityKind::Person { PERSON_MIN } else { PLACE_MIN };
            if spots.len() < floor {
                return None;
            }
            let first = spots.iter().min().map(|(index, _)| *index)?;
            Some(UnlistedName {
                form: form.to_string(),
                kind,
                count: spots.len(),
                first_node: chapters.get(first).map(|(id, _)| *id)?,
            })
        })
        .collect();
    // 稳定序：多的在前 → 人物在前（人名比地名可信）→ 写法
    out.sort_by(|a, b| {
        (b.count, a.kind != EntityKind::Person, &a.form).cmp(&(a.count, b.kind != EntityKind::Person, &b.form))
    });
    out
}

/// 把候选折成体检发现：最多 [`UNLISTED_MAX`] 条，返回 **(发现, 被截掉的条数)**。
pub fn unlisted_name_issues(names: &[UnlistedName]) -> (Vec<OutlineIssue>, usize) {
    let hidden = names.len().saturating_sub(UNLISTED_MAX);
    let issues = names.iter().take(UNLISTED_MAX).map(to_issue).collect();
    (issues, hidden)
}

/// 一条候选 → 一条发现。
///
/// **锚点留空是有意的**：忽略标记的指纹把锚点也算进去（`规则|锚点|主体`），而"这个名字
/// 第一次出现在哪一章"会随着作者改前面的章而变；作者忽略的是**这个名字**本身，
/// 不是"它在第几章"。所以定位走 `params.node_id`，锚点不动。
fn to_issue(name: &UnlistedName) -> OutlineIssue {
    OutlineIssue::new(
        IssueRule::EntityUnlistedName,
        Vec::new(),
        vec![name.form.clone()],
        [
            ("form", name.form.clone()),
            ("kind", name.kind.as_str().to_string()),
            ("count", name.count.to_string()),
            ("node_id", name.first_node.to_string()),
        ],
    )
}

/// 设定卡上已经登记过的叫法（名字 + 别称）。
fn known_tokens(cards: &[EntityCard]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for card in cards {
        for token in std::iter::once(&card.name).chain(card.aliases.iter()) {
            let token = token.trim();
            if !token.is_empty() {
                out.insert(token.to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// 这个写法是不是**已经在设定里了**（相等 / 是别人的一部分 / 别人是它的一部分）。
///
/// 双向包含是有意的：设定卡叫「青城」而正文写「青城山」时，那是同一处地方，
/// 不该报"设定里没有"（宁漏不猜）。
fn covered(form: &str, known: &[String]) -> bool {
    known
        .iter()
        .any(|token| token == form || token.contains(form) || form.contains(token))
}

/// 记一条候选（写法 → 类别 + 位置）。
fn record<'a>(
    hits: &mut BTreeMap<&'a str, (EntityKind, BTreeSet<(usize, usize)>)>,
    form: &'a str,
    kind: EntityKind,
    chapter: usize,
    start: usize,
    known: &[String],
) {
    // 单字不成名（"大人"前面一个字都没有时，那本来就是称谓不是名字）
    if form.chars().count() < 2 || covered(form, known) {
        return;
    }
    let entry = hits.entry(form).or_insert_with(|| (kind, BTreeSet::new()));
    entry.1.insert((chapter, start));
}

/// 称谓帧：正文里的称谓词 + 它左边的名字字。
fn collect_titles<'a>(
    text: &'a str,
    chapter: usize,
    known: &[String],
    hits: &mut BTreeMap<&'a str, (EntityKind, BTreeSet<(usize, usize)>)>,
) {
    for (at, first) in text.char_indices() {
        for title in PERSON_TITLES {
            if !title.starts_with(first) || !text[at..].starts_with(title) {
                continue;
            }
            let Some(start) = prefix_start(text, at) else { continue };
            record(hits, &text[start..at + title.len()], EntityKind::Person, chapter, start, known);
        }
    }
}

/// 地名帧：正文里的通名 + 它左边的名字字。
///
/// 右边紧跟着**另一个通名**时跳过：那是更长地名的中间那截（"青**城**山"里的"城"），
/// 留着它就会把"青城"报成一个地名。
fn collect_places<'a>(
    text: &'a str,
    chapter: usize,
    known: &[String],
    hits: &mut BTreeMap<&'a str, (EntityKind, BTreeSet<(usize, usize)>)>,
) {
    for (at, suffix) in text.char_indices() {
        if !PLACE_SUFFIXES.contains(suffix) {
            continue;
        }
        let end = at + suffix.len_utf8();
        if text[end..].chars().next().is_some_and(|next| PLACE_SUFFIXES.contains(next)) {
            continue;
        }
        let Some(start) = prefix_start(text, at) else { continue };
        record(hits, &text[start..end], EntityKind::Setting, chapter, start, known);
    }
}

/// 姓名帧：常用单字姓 + 一两个字，**紧跟着说话或动作词**才算。
fn collect_surnames<'a>(
    text: &'a str,
    chapter: usize,
    known: &[String],
    hits: &mut BTreeMap<&'a str, (EntityKind, BTreeSet<(usize, usize)>)>,
) {
    for (at, first) in text.char_indices() {
        if !SURNAMES.contains(first) {
            continue;
        }
        // 左边也必须是断点：不然"明白说道"里的"白说"会被当成一个名字
        if let Some((_, prev)) = text[..at].char_indices().next_back() {
            if !is_break(prev) {
                continue;
            }
        }
        // 先试三字名（姓 + 两个字），再退到两字名：长的更具体
        for len in [3usize, 2] {
            let Some(end) = after_chars(text, at, len) else { continue };
            let name = &text[at..end];
            // 名字里不许混进虚词 / 量词（"张三说"这种切法要挡住）。
            // **数字要放行**：张三 / 李四 / 王五 是正经名字（见 `name_forbidden`）。
            if name.chars().any(name_forbidden) {
                continue;
            }
            if !ACTION_MARKERS.iter().any(|marker| text[end..].starts_with(marker)) {
                continue;
            }
            record(hits, name, EntityKind::Person, chapter, at, known);
            break;
        }
    }
}


/// 名字的起点：从 `at` 往前走到**断点**为止（最多 [`PREFIX_MAX`] 个字）。
///
/// 一个字都没走到（左边就是断点）⇒ `None`（那是称谓本身，不是"名字+称谓"）；
/// 走满了 [`PREFIX_MAX`] 个却还没碰上断点 ⇒ `None`（起点不可信，见文件头）。
fn prefix_start(text: &str, at: usize) -> Option<usize> {
    let mut start = at;
    let mut steps = 0usize;
    while steps < PREFIX_MAX {
        let Some((prev, ch)) = text[..start].char_indices().next_back() else { break };
        if is_break(ch) {
            break;
        }
        start = prev;
        steps += 1;
    }
    if steps == 0 {
        return None;
    }
    if steps == PREFIX_MAX {
        if let Some((_, ch)) = text[..start].char_indices().next_back() {
            if !is_break(ch) {
                return None;
            }
        }
    }
    // 前缀只有一个字、而且那个字是修饰语（"大师兄"）⇒ 那是叫法，不是名字
    let prefix = text[start..at].chars().next()?;
    if steps == 1 && BAD_LONE_PREFIX.contains(prefix) {
        return None;
    }
    Some(start)
}

/// `at` 往后第 `n` 个字符之后的字节位置（字符不够就 `None`）。
fn after_chars(text: &str, at: usize, n: usize) -> Option<usize> {
    if text[at..].chars().count() < n {
        return None;
    }
    text[at..].char_indices().nth(n).map(|(offset, _)| at + offset).or(Some(text.len()))
}


/// 名字里**不许出现**的字：虚词 / 代词 / 量词 / 常见动作动词（"张三说"那种切法要挡住）。
///
/// **数字放行**：它们在 [`BOUNDARY`] 里只是为了"一位姑娘"能从"一"断开，
/// 而张三 / 李四 / 王五 是正经名字——把那两个字挡掉，一整类中文名就全认不出来了。
fn name_forbidden(ch: char) -> bool {
    !is_han(ch) || (BOUNDARY.contains(ch) && !NUMERALS.contains(ch))
}
/// 汉字（名字只由汉字组成；标点、空白、字母、数字一律算断点）。
fn is_han(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

/// 这个字能不能算"名字的起点边界"（不能 ⇒ 名字不该从这儿开始）。
fn is_break(ch: char) -> bool {
    !is_han(ch) || BOUNDARY.contains(ch)
}
