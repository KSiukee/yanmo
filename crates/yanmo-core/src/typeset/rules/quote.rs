//! 引号那一条：**统一到作者选的那一套**（弯引号或角引号）。
//!
//! 两个分寸：
//! - 半角引号**成对才换**：数出来是奇数就整条不动——宁可不改，也不制造没配对的引号；
//! - 英文撇号（`don't`）不是引号：只有挨着中文的那只才当引号看。

use super::{char_before, is_cjk_like, next_at, Edit};
use crate::typeset::QuoteStyle;

/// 引号统一：半角引号换成目标那一套，已经在用的中文引号也换成目标那一套。
pub(crate) fn quote(text: &str, style: QuoteStyle) -> Vec<Edit> {
    let mut out = Vec::new();
    // ① 已经在用的中文引号：不是目标那一套就换过去
    for (i, c) in text.char_indices() {
        if let Some(want) = restyle(c, style) {
            out.push(Edit::replace("quote", i, i + c.len_utf8(), &want.to_string()));
        }
    }
    // ② 半角引号：成对的才换，落单的一只留着不动
    let (open, close, single_open, single_close) = pair_marks(style);
    out.extend(pair_ascii(text, '"', open, close));
    out.extend(pair_ascii(text, '\'', single_open, single_close));
    out
}

/// 目标风格的四个引号。
fn pair_marks(style: QuoteStyle) -> (char, char, char, char) {
    match style {
        QuoteStyle::Curly => ('“', '”', '‘', '’'),
        QuoteStyle::Corner => ('「', '」', '『', '』'),
    }
}

/// 现在这个引号不是目标风格的话，换成目标风格的那一个。
fn restyle(c: char, style: QuoteStyle) -> Option<char> {
    let (open, close, single_open, single_close) = pair_marks(style);
    let from = match style {
        QuoteStyle::Curly => [('「', open), ('」', close), ('『', single_open), ('』', single_close)],
        QuoteStyle::Corner => [('“', open), ('”', close), ('‘', single_open), ('’', single_close)],
    };
    from.iter().find(|(old, _)| *old == c).map(|(_, want)| *want)
}

/// 半角引号成对换掉：第 1、3、5… 个是左引号，第 2、4、6… 个是右引号。
fn pair_ascii(text: &str, mark: char, open: char, close: char) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    for (i, c) in text.char_indices() {
        if c != mark {
            continue;
        }
        // 英文撇号（`don't`）不是引号：只有挨着中文的那只才算
        if mark == '\'' && !cjk_side(text, i) {
            continue;
        }
        let want = if seen % 2 == 0 { open } else { close };
        out.push(Edit::replace("quote", i, i + c.len_utf8(), &want.to_string()));
        seen += 1;
    }
    // 数出来是奇数＝有一只落单：整条不换
    if seen % 2 == 1 {
        out.pop();
    }
    out
}

/// 这只半角单引号的两侧（任一侧）挨着中文吗。
fn cjk_side(text: &str, at: usize) -> bool {
    char_before(text, at).is_some_and(is_cjk_like)
        || next_at(text, at + '\''.len_utf8()).is_some_and(is_cjk_like)
}
