//! 「报告」「应用」与「提醒」——dry-run 的那一半。
//!
//! 输出分两类，界线是**能不能确定怎么改**：
//! - [`Change`]：能确定怎么改的（省略号、衍字、半角标点……），列出来让作者逐条勾，勾了才改；
//! - [`Notice`]：**不能确定怎么改**的（引号括号缺一半——补哪边都是猜），只报告，不动正文。
//!
//! `apply` 只认勾中的那几处，且**一次全改或一次都不改**：序号对不上就整批拒绝，
//! 由界面重新预览。改一半比不改更糟——作者会对不上账。

use std::collections::BTreeSet;

use serde::Serialize;

use super::rules::{self, Edit};
use super::Options;
use crate::error::codes;
use crate::error::{Error, Result};

/// 给作者看的一处改动（能确定怎么改的）。
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    /// 哪条规则提出来的（界面对着自己的字典取名字，核心不给句子）
    pub rule: &'static str,
    /// 改前那一小段（原文里的样子）
    pub before: String,
    /// 改后（插入类改动这里是那个空格）
    pub after: String,
    /// 落在第几段（从 1 起）——作者按它找得回去
    pub paragraph: usize,
    /// 前文几个字（定位用）
    pub context_before: String,
    /// 后文几个字
    pub context_after: String,
}

/// 给作者看的一处**提醒**：只报告，不给改法。
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    /// 哪个检查提出来的（`pair_missing`）
    pub rule: &'static str,
    /// 哪一对符号（`quote_double` / `corner` / `paren` / `title`…），界面拿去对表取名字
    pub mark: &'static str,
    /// `unclosed`（开着的没关）/ `unopened`（关的那个没有开）——句子在界面字典里
    pub side: &'static str,
    /// 落在第几段（从 1 起）
    pub paragraph: usize,
    /// 前文几个字（定位用）
    pub context_before: String,
    /// 后文几个字
    pub context_after: String,
}

/// 一次扫描的结果：能改的与只能提醒的，分开放。
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub changes: Vec<Change>,
    pub notices: Vec<Notice>,
}

/// 扫描（dry-run）：**只报告，不改一个字**。
pub fn scan(text: &str, options: &Options) -> Result<Report> {
    let style = options.quote_style()?;
    let changes =
        rules::collect(text, style).iter().map(|edit| describe(text, edit)).collect();
    let notices = rules::pair::pair_missing(text).iter().map(|spot| note(text, spot)).collect();
    Ok(Report { changes, notices })
}

/// 按作者勾中的序号应用；序号是 [`scan`] 返回的那一份 `changes` 里的下标。
///
/// 有一个序号对不上，就整批不应用并报 [`codes::TYPESET_CHANGE_UNKNOWN`]。
pub fn apply(text: &str, options: &Options, accepted: &[usize]) -> Result<String> {
    let style = options.quote_style()?;
    let edits = rules::collect(text, style);
    let mut chosen: Vec<&Edit> = Vec::with_capacity(accepted.len());
    for index in accepted.iter().copied().collect::<BTreeSet<_>>() {
        let edit = edits.get(index).ok_or_else(|| {
            Error::invalid_with(codes::TYPESET_CHANGE_UNKNOWN, [("index", index.to_string())])
        })?;
        chosen.push(edit);
    }
    // 从后往前换：前面的区间不会因为后面的替换而挪位
    let mut out = text.to_string();
    for edit in chosen.iter().rev() {
        out.replace_range(edit.start..edit.end, &edit.text);
    }
    Ok(out)
}

/// 把一条改动讲清楚：在第几段、前后是什么。
fn describe(text: &str, edit: &Edit) -> Change {
    let (paragraph, context_before, context_after) = locate(text, edit.start, edit.end);
    Change {
        rule: edit.rule,
        before: text[edit.start..edit.end].to_string(),
        after: edit.text.clone(),
        paragraph,
        context_before,
        context_after,
    }
}

/// 把一处提醒讲清楚（同样给段落与上下文，好让作者找得到）。
fn note(text: &str, spot: &rules::pair::Sighting) -> Notice {
    let width = text[spot.at..].chars().next().map(char::len_utf8).unwrap_or(0);
    let (paragraph, context_before, context_after) = locate(text, spot.at, spot.at + width);
    Notice {
        rule: "pair_missing",
        mark: spot.mark,
        side: spot.side,
        paragraph,
        context_before,
        context_after,
    }
}

/// 这一处在第几段、前后各是什么。
fn locate(text: &str, start: usize, end: usize) -> (usize, String, String) {
    // 段落按空行分（与正文的存储口径一致），不是按换行数——作者数的是段。
    // 最后一块是"当前这段的前半截"，不算一整段，所以先弹掉。
    let mut blocks: Vec<&str> = text[..start].split("\n\n").collect();
    blocks.pop();
    let paragraph = blocks.iter().filter(|block| !block.trim().is_empty()).count() + 1;
    let para_start = text[..start].rfind('\n').map(|at| at + 1).unwrap_or(0);
    let para_end = text[end..].find('\n').map(|at| end + at).unwrap_or(text.len());
    (paragraph, tail(&text[para_start..start]), head(&text[end..para_end]))
}

/// 前后各留几个字：够作者认出在哪儿，又不会把整段搬给界面。
const CONTEXT: usize = 12;

fn head(text: &str) -> String {
    text.chars().take(CONTEXT).collect()
}

fn tail(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().rev().take(CONTEXT).collect();
    chars.reverse();
    chars.into_iter().collect()
}
