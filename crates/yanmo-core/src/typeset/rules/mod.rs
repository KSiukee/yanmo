//! 七条规则：每条都只做一件事——读一段文本、列出**它想改的地方**，自己一个字都不改。
//!
//! `Edit` 用**字节区间**（`start..end`）表达：「换掉这一段」是 `end > start`，
//! 「在这里插一个字符」是 `start == end`。所有区间都落在字符边界上。

pub(crate) mod pair;
pub(crate) mod punct;
pub(crate) mod quote;
pub(crate) mod space;
pub(crate) mod typo;

use crate::typeset::QuoteStyle;

/// 一条待改的地方：把 `start..end` 换成 `text`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    /// 哪个规则提出来的（稳定代码）
    pub rule: &'static str,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Edit {
    pub(crate) fn replace(rule: &'static str, start: usize, end: usize, text: &str) -> Self {
        Self { rule, start, end, text: text.to_string() }
    }

    pub(crate) fn insert(rule: &'static str, at: usize, text: &str) -> Self {
        Self { rule, start: at, end: at, text: text.to_string() }
    }
}

/// 把七条规则的提议收成一份清单，并按**文档顺序**排好。
///
/// 同一处被两条规则看上时，按 [`crate::typeset::RULES`] 的先后定夺（先提的赢）——
/// 例如 `。。。` 只该收成省略号，不该再被"连续标点"动一次。
pub(crate) fn collect(text: &str, style: QuoteStyle) -> Vec<Edit> {
    let mut out: Vec<Edit> = Vec::new();
    let mut claimed: Vec<(usize, usize)> = Vec::new();
    let proposals = [
        punct::ellipsis(text),
        punct::dash(text),
        space::leading_space(text),
        typo::repeat_char(text),
        typo::repeat_word(text),
        punct::repeat_punct(text),
        punct::halfwidth_punct(text),
        quote::quote(text, style),
        space::cjk_latin_space(text),
    ];
    for edits in proposals {
        for edit in edits {
            if claimed.iter().any(|(start, end)| overlaps(&edit, *start, *end)) {
                continue;
            }
            claimed.push((edit.start, edit.end));
            out.push(edit);
        }
    }
    out.sort_by_key(|edit| (edit.start, edit.end));
    out
}

/// 两处改动是否打架（空区间＝插入，只有真落在别人中间才算）。
fn overlaps(edit: &Edit, start: usize, end: usize) -> bool {
    let (a_start, a_end) = (edit.start, edit.end);
    match (a_start == a_end, start == end) {
        (true, true) => a_start == start,
        (true, false) => a_start > start && a_start < end,
        (false, true) => start > a_start && start < a_end,
        (false, false) => a_start < end && start < a_end,
    }
}

/// 算不算「汉字系」：中文标点该不该用全角、中英之间该不该留白，都看它。
pub(crate) fn is_cjk_like(c: char) -> bool {
    crate::text::is_cjk(c) || crate::text::is_kana_or_hangul(c) || is_cjk_punct(c)
}

/// 中文标点与全角符号（含弯引号、破折号、省略号）。
pub(crate) fn is_cjk_punct(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}'   // 全角空格、、。《》「」『』等
        | '\u{FF01}'..='\u{FF0F}' // ！＂＃…）等全角标点前半段
        | '\u{FF1A}'..='\u{FF20}'
        | '\u{FF3B}'..='\u{FF40}'
        | '\u{FF5B}'..='\u{FF65}'
        | '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' // ‘ ’ “ ”
        | '\u{2014}' | '\u{2013}'                            // — –
        | '\u{2026}'                                         // …
    )
}

/// 半角引号：判断"上一个有意义的字符"时它们要透明（`他说"你好",` 里的逗号仍算中文语境）。
pub(crate) fn is_ascii_quote(c: char) -> bool {
    c == '"' || c == '\''
}

/// ASCII 字母或数字——小数、版本号、英文缩写靠它区分开。
pub(crate) fn is_ascii_word(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// 连着 `c` 的区间末尾。
pub(crate) fn run_end(text: &str, at: usize, c: char) -> usize {
    let mut end = at;
    while text[end..].starts_with(c) {
        end += c.len_utf8();
    }
    end
}

pub(crate) fn char_before(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

pub(crate) fn next_at(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

/// `at` 处的那个字符——调用方走的都是自建的字符边界，取不到就说明实现有错。
pub(crate) fn char_at(text: &str, at: usize) -> char {
    // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
    next_at(text, at).expect("字节位置落在字符边界上")
}

/// 改动两侧紧挨着 ASCII 字母/数字（缩写、代码、参数都是这个形状）。
pub(crate) fn word_neighbours(text: &str, start: usize, end: usize) -> bool {
    char_before(text, start).is_some_and(is_ascii_word) || next_at(text, end).is_some_and(is_ascii_word)
}

/// 改动两侧都是中文（判断 `–` 是不是破折号用）。
pub(crate) fn cjk_neighbours(text: &str, start: usize, end: usize) -> bool {
    char_before(text, start).is_some_and(is_cjk_like) && next_at(text, end).is_some_and(is_cjk_like)
}
