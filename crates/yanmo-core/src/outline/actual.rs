//! 「计划 vs 实际」的**判据**：正文里到底出现了谁、哪条伏笔像被点到了。
//!
//! 纯函数、吃值吐发现，不碰库也不产句子（句子在界面字典 `actual.*` 里）。
//!
//! # 只做机械可判的，绝不猜
//!
//! 判"正文写的和章纲对不上"要**读懂正文**——那件事这一层不做（见 `outline/mod.rs` 里
//! 为什么先做规则版）。这里只做两件**字面上判得动**的事：
//!
//! | 判据 | 怎么判 | 认不出的时候 |
//! |---|---|---|
//! | 出场人物 | 设定卡的名字 / 别称在正文里**字面出现** | 不算出现（宁可漏报，不猜） |
//! | 伏笔 | 这条伏笔的正文里有 ≥2 个三字片段在正文里出现 | 不算命中（候选，不是结论） |
//!
//! **四格（视角 / 目标 / 冲突 / 结果）与「一句话」判不了**，界面上只并排摆出来给人看。
//!
//! # 最长优先：防"阿明"把"小明"顶掉
//!
//! 两张卡分别叫"小明"与"明"时，正文里的"小明"字面上两个名字都在。若都算"出现过"，
//! 作者会看到一张根本没写的卡被报成写了。所以：**长名字先占位**，一个短名字的所有出现
//! 位置都被更长的名字盖住时，它不算出现（`shadowed` 里能看见被盖掉的那几个，便于核对）。

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{EntityCard, Foreshadow, ForeshadowState};

/// 一条伏笔最多取几个片段（够判"像不像"，再多只是白扫）。
const FORESHADOW_FRAGMENTS: usize = 8;
/// 伏笔正文短到这个长度（含）就整条当一个"关键词"。
const SHORT_FORESHADOW: usize = 3;
/// 片段长度（三字：短到能在改写过的句子里还认得出来，长到不至于满篇都是）。
const FRAGMENT_LEN: usize = 3;
/// 至少这么多个不同片段在正文里出现，才算"像被点到了"。
const FORESHADOW_MIN_HITS: usize = 2;

/// 一张设定卡在正文里被提到的事实（给界面显示用）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CastRef {
    pub entity_id: i64,
    pub name: String,
    /// 命中的那个叫法（可能不是正式名，而是别称）
    pub matched: String,
}

/// 一条"像被点到了"的伏笔（**候选**，不是结论）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ForeshadowHit {
    pub foreshadow_id: i64,
    pub body: String,
    /// 命中的片段（让作者看得见"凭什么说像"）
    pub fragments: Vec<String>,
    /// 这条伏笔一共取了几个片段
    pub fragments_total: usize,
}

/// 一章的"计划 vs 实际"。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChapterActual {
    pub node_id: i64,
    pub title: String,
    /// 三档：未写 / 已写 / 偏离（判据见 [`chapter_state`]）
    pub state: ChapterState,
    /// 这一章有没有正文（"未写"那一档全靠它）
    pub has_body: bool,
    /// 计划里挂着的出场人物
    pub planned: Vec<CastRef>,
    /// 正文里字面出现、而且**计划里也挂着**的
    pub matched: Vec<CastRef>,
    /// 正文里字面出现、**计划里没有**的（可以一键补进计划）
    pub extra: Vec<CastRef>,
    /// 计划里挂着、**正文里没认到**的（只提示，绝不自动撤）
    pub missing: Vec<CastRef>,
    /// 像被点到的伏笔（候选；这一版只报不动）
    pub foreshadows: Vec<ForeshadowHit>,
}

/// 正文里出现了哪些卡（`names_in_text` 的规格化入口）。
///
/// `cards` 是这本书的全部设定卡；返回**按卡 id 稳定排序**。
pub fn mentioned_cards(text: &str, cards: &[EntityCard]) -> Vec<CastRef> {
    // 每个名字属于哪张卡：一个字面可能被两张卡共用（体检会报那种冲突），
    // 这里两个都算出现——**谁都没写错，是名字真撞了**。
    let mut owners: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    for card in cards {
        let mut names = vec![card.name.clone()];
        names.extend(card.aliases.iter().cloned());
        for name in names {
            let name = name.trim().to_string();
            if !name.is_empty() {
                owners.entry(name).or_default().push((card.id, card.name.clone()));
            }
        }
    }
    let names: Vec<(String, Vec<(i64, String)>)> = owners.into_iter().collect();

    let haystack = text.to_lowercase();
    // 每个名字出现的字符位置（按字符数，不按字节——中文一个字符三字节，混着算会错位）
    let mut spans: Vec<(usize, usize, &str, &Vec<(i64, String)>)> = Vec::new();
    for (name, holders) in &names {
        let needle = name.to_lowercase();
        let length = needle.chars().count();
        for (start, _) in char_matches(&haystack, &needle) {
            spans.push((start, start + length, name.as_str(), holders));
        }
    }

    // 最长优先占位：一个短名字的所有出现都被更长的名字盖住时，它不算出现
    let mut out = Vec::new();
    for (start, end, name, holders) in &spans {
        let shadowed = spans.iter().any(|(other_start, other_end, other_name, _)| {
            // 更长的名字（或同样长但按字典序在前，避免两个等长名字互相盖）
            let longer = (other_end - other_start) > (end - start)
                || ((other_end - other_start) == (end - start) && *other_name < *name);
            longer && other_start <= start && end <= other_end
        });
        if shadowed {
            continue;
        }
        for (entity_id, card_name) in holders.iter() {
            out.push(CastRef {
                entity_id: *entity_id,
                name: card_name.clone(),
                matched: (*name).to_string(),
            });
        }
    }
    out.sort_by(|a, b| (a.entity_id, &a.matched).cmp(&(b.entity_id, &b.matched)));
    out.dedup();
    out
}

/// 正文里像被点到的伏笔（只看**还埋着**的：收掉的不用再念）。
///
/// 判据：把伏笔正文去掉空白与标点之后，取三字片段（短于四字的整条当一个片段），
/// **有两个以上不同片段在正文里出现**才算命中（阈值见 `FORESHADOW_MIN_HITS`）。
/// 它是一句"字面像"，**不是"这一章真收了"**——所以叫候选，界面上也这么写。
pub fn foreshadow_candidates(text: &str, foreshadows: &[Foreshadow]) -> Vec<ForeshadowHit> {
    let haystack = normalize_for_match(text);
    if haystack.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for foreshadow in foreshadows {
        if foreshadow.state != ForeshadowState::Planted {
            continue; // 收掉 / 不写了的，不再念
        }
        let fragments = fragments_of(&foreshadow.body);
        if fragments.is_empty() {
            continue;
        }
        let hit: Vec<String> = fragments
            .iter()
            .filter(|fragment| haystack.contains(fragment.as_str()))
            .cloned()
            .collect();
        let enough = if fragments.len() == 1 {
            !hit.is_empty() // 整条就是一个短词：出现即算
        } else {
            hit.len() >= FORESHADOW_MIN_HITS
        };
        if enough {
            out.push(ForeshadowHit {
                foreshadow_id: foreshadow.id,
                body: foreshadow.body.clone(),
                fragments: hit,
                fragments_total: fragments.len(),
            });
        }
    }
    out.sort_by_key(|hit| hit.foreshadow_id);
    out
}

/// 这一章的**计划是不是一字没填**（章纲 / 四格 / 出场人物 / 这一章的伏笔账都没有）。
///
/// 它判的是"作者还没打算写这一章"，与"计划填了但正文没写到"是两件事——
/// 前者不该报成偏离，后者才要。
pub fn plan_is_empty(
    summary: &str,
    fields: &crate::model::SceneFields,
    planned_cast: usize,
    planted_or_collected: usize,
) -> bool {
    summary.trim().is_empty()
        && fields.pov.trim().is_empty()
        && fields.goal.trim().is_empty()
        && fields.conflict.trim().is_empty()
        && fields.outcome.trim().is_empty()
        && planned_cast == 0
        && planted_or_collected == 0
}

/// 一章的三档（界面按它给颜色与说法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChapterState {
    /// 计划填了、正文还是空的
    Unwritten,
    /// 有正文，计划与正文里认得到的东西都对得上
    Written,
    /// 有正文，但跟计划对不上（一字没填 / 有人多出来 / 有人没认到）
    Deviated,
}

/// 判这一章算哪一档。
///
/// **两样都空**（没计划也没正文）返回 `None`：那是"还没轮到这一章"，报出来只会淹掉真问题。
pub fn chapter_state(
    plan_empty: bool,
    has_body: bool,
    extra: usize,
    missing: usize,
) -> Option<ChapterState> {
    match (plan_empty, has_body) {
        (true, false) => None,
        (false, false) => Some(ChapterState::Unwritten),
        (true, true) => Some(ChapterState::Deviated),
        (false, true) => {
            if extra == 0 && missing == 0 {
                Some(ChapterState::Written)
            } else {
                Some(ChapterState::Deviated)
            }
        }
    }
}

/// 一个字面在 `haystack`（已小写）里出现的**字符**位置。
fn char_matches(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // 按字节滑动再折算成字符位置：needle 都是短词，这样比按字符构造子串便宜。
    let mut offset = 0usize;
    while let Some(found) = haystack[offset..].find(needle) {
        let bytes = offset + found;
        out.push((haystack[..bytes].chars().count(), bytes));
        // 从命中处往后移一个字符再找（防止死循环；重叠出现也要认）
        offset = bytes + haystack[bytes..].chars().next().map(char::len_utf8).unwrap_or(1);
        if offset >= haystack.len() {
            break;
        }
    }
    out
}

/// 比对用的规格化：去掉空白与常见标点（"玉佩，上的裂纹"与"玉佩上的裂纹"该算同一句）。
fn normalize_for_match(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !is_punctuation(*c))
        .collect()
}

fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c, '，' | '。' | '、' | '；' | '：' | '？' | '！' | '“' | '”' | '‘' | '’'
            | '（' | '）' | '《' | '》' | '—' | '…' | '·' | '「' | '」' | '『' | '』')
}

/// 一条伏笔取出来的"关键词"片段（去重、最多 [`FORESHADOW_FRAGMENTS`] 个）。
fn fragments_of(body: &str) -> Vec<String> {
    let text = normalize_for_match(body);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    if chars.len() <= SHORT_FORESHADOW {
        return vec![text];
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for window in chars.windows(FRAGMENT_LEN) {
        let fragment: String = window.iter().collect();
        if seen.insert(fragment.clone()) {
            out.push(fragment);
            if out.len() >= FORESHADOW_FRAGMENTS {
                break;
            }
        }
    }
    out
}
