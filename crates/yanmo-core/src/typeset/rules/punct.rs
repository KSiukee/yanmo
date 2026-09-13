//! 标点类规则：省略号、破折号、连续标点、半角标点。
//!
//! 共同的分寸：**只动"几乎不可能有第二种意思"的地方**。英文缩写、小数、版本号、
//! 代码参数（`e.g.` / `3.14` / `--self-test`）一律绕开——宁可漏改，不可错改。
//! 引号单列一条（它要多一套"用哪一套引号"的偏好，见 [`super::quote`]）。

use super::{
    char_at, cjk_neighbours, is_ascii_quote, is_ascii_word, is_cjk_like, run_end,
    word_neighbours, Edit,
};

/// 省略号：`...`、`。。。`、单个 `…` 三种写法都收成规范的 `……`。
pub(crate) fn ellipsis(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        match char_at(text, i) {
            '.' => {
                let end = run_end(text, i, '.');
                // 两个点是缩写（`e.g.`），三个以上才是省略号
                if end - i >= 3 {
                    out.push(Edit::replace("ellipsis", i, end, "……"));
                }
                i = end;
            }
            '。' => {
                let end = run_end(text, i, '。');
                // 连着两个以上的句号是省略号打错了；单个句号不动
                if end - i >= 2 * '。'.len_utf8() {
                    out.push(Edit::replace("ellipsis", i, end, "……"));
                }
                i = end;
            }
            '…' => {
                let end = run_end(text, i, '…');
                // 规范是一组两点：一个、三个以上都收成两个
                if end - i != 2 * '…'.len_utf8() {
                    out.push(Edit::replace("ellipsis", i, end, "……"));
                }
                i = end;
            }
            c => i += c.len_utf8(),
        }
    }
    out
}

/// 破折号：中文破折号占两格，`--` / 单个 `—` 都收成 `——`。
pub(crate) fn dash(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        match char_at(text, i) {
            '-' => {
                let end = run_end(text, i, '-');
                // 代码与命令行参数里的双连字符（`--self-test`）不动
                if end - i >= 2 && !word_neighbours(text, i, end) {
                    out.push(Edit::replace("dash", i, end, "——"));
                }
                i = end;
            }
            '—' => {
                let end = run_end(text, i, '—');
                if end - i != 2 * '—'.len_utf8() {
                    out.push(Edit::replace("dash", i, end, "——"));
                }
                i = end;
            }
            '–' => {
                // 半角横线只在两边都是中文时才算破折号（`1–2` 是范围，不动）
                if cjk_neighbours(text, i, i + '–'.len_utf8()) {
                    out.push(Edit::replace("dash", i, i + '–'.len_utf8(), "——"));
                }
                i += '–'.len_utf8();
            }
            c => i += c.len_utf8(),
        }
    }
    out
}

/// 连着打的标点收成一个（`？？` → `？`）。**网文里常是故意的**，所以在"要作者自己勾"那一档。
pub(crate) fn repeat_punct(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    let mut i = 0;
    while i < text.len() {
        let c = char_at(text, i);
        let end = run_end(text, i, c);
        if let Some(full) = repeatable(c) {
            let count = (end - i) / c.len_utf8();
            if count >= 2 {
                // 中文语境里连着打的半角标点顺手收成全角（`??` → `？`）
                let keep = if c.is_ascii() && prev.is_some_and(is_cjk_like) { full } else { c };
                out.push(Edit::replace("repeat_punct", i, end, &keep.to_string()));
            }
        }
        if !c.is_whitespace() {
            prev = Some(c);
        }
        i = end;
    }
    out
}

/// 中文语境里的半角标点换成全角：`他说,好.` → `他说，好。`。
pub(crate) fn halfwidth_punct(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    // 刚换过一个左括号：它的右括号要跟着换，否则会留下半中半西的一对括号
    let mut open_paren = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|(_, next)| *next);
        let width = c.len_utf8();
        match c {
            ',' | '.' | '?' | '!' | ':' | ';' => {
                // 前一个字是中文、后面不是英文或数字（`3.14`、`中文:abc` 都因此绕开）
                if prev.is_some_and(is_cjk_like) && !next.is_some_and(is_ascii_word) {
                    out.push(Edit::replace("halfwidth_punct", i, i + width, &to_fullwidth(c).to_string()));
                }
            }
            '(' => {
                // 括号里是中文才换（`这是(English)` 保持原样，半中半西的一对比全角更难看）
                open_paren = prev.is_some_and(is_cjk_like) && bracket_holds_cjk(text, i);
                if open_paren {
                    // i18n-allow-next-line: 这是**写进正文的全角括号**（正文内容本身），不是界面文案
                    out.push(Edit::replace("halfwidth_punct", i, i + width, "（"));
                }
            }
            ')' => {
                if open_paren {
                    // i18n-allow-next-line: 同上，这是正文内容
                    out.push(Edit::replace("halfwidth_punct", i, i + width, "）"));
                }
                open_paren = false;
            }
            _ => {}
        }
        // 空白与半角引号对"上一个有意义的字符"透明：`他说"你好",` 里的逗号仍算中文语境
        if !c.is_whitespace() && !(is_ascii_quote(c) && prev.is_some_and(is_cjk_like)) {
            prev = Some(c);
        }
    }
    out
}

/// 这个左括号到它配对的右括号之间是中文吗（没配对视为"不是"，宁可不动）。
fn bracket_holds_cjk(text: &str, at: usize) -> bool {
    let rest = &text[at + 1..];
    let Some(close) = rest.find(')') else { return false };
    rest[..close].chars().any(is_cjk_like)
}

/// 允许"收成一个"的标点，以及它在中文语境里该用的全角形态。
/// `。`（交给省略号那条）、`…`、`—` 都排除在外。
fn repeatable(c: char) -> Option<char> {
    match c {
        '，' | '、' | '？' | '！' | '：' | '；' => Some(c),
        ',' => Some('，'),
        '?' => Some('？'),
        '!' => Some('！'),
        ':' => Some('：'),
        ';' => Some('；'),
        _ => None,
    }
}

/// 半角标点对应的全角形态。
fn to_fullwidth(c: char) -> char {
    match c {
        ',' => '，',
        '.' => '。',
        '?' => '？',
        '!' => '！',
        ':' => '：',
        ';' => '；',
        other => other,
    }
}
