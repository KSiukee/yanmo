//! 成稿（`work.json`）的**读**：解析、严格校验、算账、与备份清单对账。
//!
//! 这一层是**纯函数**——不碰数据库、不碰文件，只认一段文字（成稿 JSON 的内容），
//! 所以能脱离数据库直接单测。它住在 `store` 这一层，是因为成稿 JSON 是**存储层自己的格式**：
//! 写那一半是 [`super::export`]，装进备份包是 [`super::backup`]，
//! 读回来写进库是 [`super::import`]——四件事同一个格式，改动理由一样。
//!
//! # 三条规矩
//!
//! 1. **只认成稿，不认"看起来像成稿"**：少一格、多一格、类型不对，一律当场拒绝。
//!    宁可让作者拿着坏 JSON 来问，也不要默默导进去半本看不懂的书
//!    （那种"半本"最危险：作者以为救回来了，其实少了东西）；
//! 2. **一个字都不许猜**：成稿里没有的东西（每章一句话、版本历史、创作时间）不补、不编；
//! 3. **对账要能说清差在哪**：与备份清单比章节数、三口径字数、分章文本指纹，
//!    对不上的地方逐条列出来（字段名 + 清单值 + 成稿里算出来的值），不是一句"对不上"。
//!
//! # 为什么"原文"要跟着成稿走
//!
//! 标题里的号是**按位置算**的（见 [`crate::numbering`]）：作者存的是模板 `第{$N}章 灯`，
//! 显示出来才是 `第3章 灯`。成稿因此同时带 `title`（显示名）与 `title_template`
//! （原文，只在两者不同时才有这一格）。读回来时优先用原文——只带显示名的话，
//! 这本书的号会在导入那一刻**钉死成文字**，之后插一章都不会重排。
//!
//! `naming` / `language` 同理：**还没起名的卷**要按编号档才算得出名字
//! （中文档给「第一卷」、补零档给「第001卷」），少了它就只能按新库的默认档渲染。

use serde::Serialize;

use crate::error::{codes, Error, Result};
use crate::model::{NamingStyle, NodeKind, WorkKind, WorkLanguage};
use super::WorkStamp;

/// 成稿 JSON 认得的字段：**多一格都不行**（多出来的多半是"另一种东西"，别硬当自己人）。
const WORK_FIELDS: [&str; 5] = ["title", "kind", "language", "naming", "nodes"];
const NODE_FIELDS: [&str; 5] = ["kind", "title", "title_template", "body", "children"];

/// 一本成稿**写着什么**（还没进库）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDraft {
    pub title: String,
    pub kind: WorkKind,
    /// 成稿里记着的作品语言（更老的成稿没有这一格 → `None`，由调用方定）
    pub language: Option<WorkLanguage>,
    /// 成稿里记着的编号档（同上）。它决定"还没起名的卷"显示成「第1卷」还是「第一卷」
    pub naming: Option<NamingStyle>,
    pub nodes: Vec<NodeDraft>,
}

/// 一个节点的成稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDraft {
    pub kind: NodeKind,
    /// **要写进库的那一份原文**（可能是模板 `第{$N}章 灯`；空串＝作者没起过名）
    pub title: String,
    /// 原文与显示名不一样（＝这一条的号是**算出来的**，不是钉死的文字）
    pub numbered: bool,
    /// 成稿里的正文（成稿没这一格就是没写过）
    pub body: Option<String>,
    pub children: Vec<NodeDraft>,
}

/// 一份成稿的规模账——**干跑与导入后用同一份口径算**，才谈得上跟备份清单对账。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct DraftScale {
    pub nodes: i64,
    pub chapters: i64,
    /// 有正文的节点数（空章不算）
    pub bodies: i64,
    pub word_count: i64,
    pub char_count: i64,
    pub chars_no_punct: i64,
    /// 原文与显示名不一样的节点数（＞0 说明"号跟着位置走"这件事活着）
    pub templated: i64,
}

/// 一处"跟清单对不上"：**给字段名与两个取值，不给句子**（界面自己查字典说话）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StampMismatch {
    pub field: String,
    /// 清单里记的
    pub expected: String,
    /// 从成稿里算出来的
    pub actual: String,
}

/// 解析 + **严格校验**一份成稿 JSON。
pub fn parse_work_json(text: &str) -> Result<WorkDraft> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| bad("json", &error.to_string().replace(['"', '\n'], " ")))?;
    let object = value.as_object().ok_or_else(|| bad("json", "not an object"))?;
    known(object.keys().map(String::as_str), &WORK_FIELDS, "")?;
    let title = text_field(object, "title", "")?;
    let kind = WorkKind::parse(&text_field(object, "kind", "")?)?;
    let language = match object.get("language") {
        None => None,
        Some(raw) => Some(WorkLanguage::parse(&asi(raw, "language")?)?),
    };
    let naming = match object.get("naming") {
        // 认不出来的档位**当没记过**（与偏好读出来时的规矩一致，见 super::appearance）
        None => None,
        Some(raw) => NamingStyle::parse(&asi(raw, "naming")?),
    };
    let raw_nodes = array_field(object, "nodes", "")?;
    let mut nodes = Vec::with_capacity(raw_nodes.len());
    for (index, node) in raw_nodes.iter().enumerate() {
        nodes.push(parse_node(node, &format!("nodes[{index}]"), 1)?);
    }
    Ok(WorkDraft { title, kind, language, naming, nodes })
}

impl WorkDraft {
    /// 这份成稿的规模账（干跑时给作者看的数）。
    pub fn scale(&self) -> DraftScale {
        let mut scale = DraftScale::default();
        for node in &self.nodes {
            count_node(node, &mut scale);
        }
        scale
    }

    /// 分章文本指纹——与 [`crate::store::Store::draft_fingerprint`] 同一口径
    /// （按阅读顺序把每章正文拼起来做摘要）。
    pub fn fingerprint(&self) -> String {
        let mut combined = String::new();
        for node in &self.nodes {
            collect_text(node, &mut combined);
        }
        crate::text::content_hash(&combined)
    }

    /// 与清单里那一本对账（拿成稿算的数）。
    ///
    /// 写库之后请用**库里的读数**对账（[`DraftScale::compare`]）——两条路的差异是刻意的：
    /// 干跑对的是"这份 JSON 里有什么"，写库后对的是"库里真有什么"。
    pub fn compare(&self, stamp: &WorkStamp) -> Vec<StampMismatch> {
        self.scale().compare(stamp, &self.fingerprint())
    }
}

impl DraftScale {
    /// 与备份清单里那一本的账对一遍：**章节数 / 三口径字数 / 分章文本指纹**。
    ///
    /// `fingerprint` 是这一份的分章文本指纹（干跑时按成稿算，写库后按库里重渲染算——
    /// 同一口径，见 [`Store::draft_fingerprint`]）。
    pub fn compare(&self, stamp: &WorkStamp, fingerprint: &str) -> Vec<StampMismatch> {
        let mut out = Vec::new();
        for (field, expected, actual) in [
            ("chapters", stamp.chapters, self.chapters),
            ("word_count", stamp.word_count, self.word_count),
            ("chars_no_punct", stamp.chars_no_punct, self.chars_no_punct),
        ] {
            if expected != actual {
                out.push(StampMismatch {
                    field: field.to_string(),
                    expected: expected.to_string(),
                    actual: actual.to_string(),
                });
            }
        }
        if stamp.fingerprint != fingerprint {
            out.push(StampMismatch {
                field: "fingerprint".to_string(),
                expected: stamp.fingerprint.clone(),
                actual: fingerprint.to_string(),
            });
        }
        out
    }
}

fn parse_node(value: &serde_json::Value, at: &str, depth: usize) -> Result<NodeDraft> {
    if depth > super::MAX_TREE_DEPTH {
        return Err(bad(at, "too deep"));
    }
    let object = value.as_object().ok_or_else(|| bad(at, "not an object"))?;
    known(object.keys().map(String::as_str), &NODE_FIELDS, at)?;
    let kind = NodeKind::parse(&text_field(object, "kind", at)?)?;
    let shown = text_field(object, "title", at)?;
    // 带了原文那一格就用原文（空串也是有效取值：那是"还没起名的卷"）；没带＝原文与显示名一致
    let title = match object.get("title_template") {
        Some(raw) => asi(raw, &format!("{at}.title_template"))?,
        None => shown.clone(),
    };
    let body = match object.get("body") {
        None => None,
        Some(raw) => {
            if !kind.holds_body() {
                return Err(bad(&format!("{at}.body"), "kind holds no body"));
            }
            Some(asi(raw, &format!("{at}.body"))?)
        }
    };
    let children = match object.get("children") {
        None => Vec::new(),
        Some(raw) => {
            let items = raw.as_array().ok_or_else(|| bad(&format!("{at}.children"), "not an array"))?;
            let mut out = Vec::with_capacity(items.len());
            for (index, child) in items.iter().enumerate() {
                out.push(parse_node(child, &format!("{at}.children[{index}]"), depth + 1)?);
            }
            out
        }
    };
    Ok(NodeDraft { numbered: title != shown, kind, title, body, children })
}

fn count_node(node: &NodeDraft, scale: &mut DraftScale) {
    scale.nodes += 1;
    if node.kind == NodeKind::Chapter {
        scale.chapters += 1;
    }
    if node.numbered {
        scale.templated += 1;
    }
    if let Some(body) = &node.body {
        scale.bodies += 1;
        let stats = super::content::stats_of(body);
        scale.word_count += stats.word_count;
        scale.char_count += stats.char_count;
        scale.chars_no_punct += stats.chars_no_punct;
    }
    for child in &node.children {
        count_node(child, scale);
    }
}

fn collect_text(node: &NodeDraft, out: &mut String) {
    if let Some(body) = &node.body {
        // 换行归一与本项目"一份内容写成文件"的口径同一处（见 super::export）
        out.push_str(&crate::store::normalize(body));
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

/// 一处不合格：**位置 + 技术细节**（都是 ASCII，界面不用翻译它们）。
fn bad(field: &str, detail: &str) -> Error {
    Error::invalid_with(
        codes::IMPORT_DRAFT_INVALID,
        [("field", field.to_string()), ("detail", detail.to_string())],
    )
}

/// 字段名单之外的一律拒绝（`at` 为空＝顶层）。
fn known<'a>(fields: impl Iterator<Item = &'a str>, allowed: &[&str], at: &str) -> Result<()> {
    for field in fields {
        if !allowed.contains(&field) {
            let where_ = if at.is_empty() { field.to_string() } else { format!("{at}.{field}") };
            return Err(bad(&where_, "unknown field"));
        }
    }
    Ok(())
}

fn asi(value: &serde_json::Value, field: &str) -> Result<String> {
    value.as_str().map(str::to_string).ok_or_else(|| bad(field, "not a string"))
}

fn text_field(
    object: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    at: &str,
) -> Result<String> {
    let where_ = if at.is_empty() { name.to_string() } else { format!("{at}.{name}") };
    match object.get(name) {
        None => Err(bad(&where_, "missing")),
        Some(value) => asi(value, &where_),
    }
}

fn array_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
    at: &str,
) -> Result<&'a Vec<serde_json::Value>> {
    let where_ = if at.is_empty() { name.to_string() } else { format!("{at}.{name}") };
    match object.get(name) {
        None => Err(bad(&where_, "missing")),
        Some(value) => value.as_array().ok_or_else(|| bad(&where_, "not an array")),
    }
}
