//! 空白类规则：段首多余的空白、中英之间的留白。

use super::Edit;
use crate::text;

/// 段首多余的空白（全角空格、半角空格、制表符）**删掉**：缩进交给样式层。
///
/// 这是从别处粘贴稿子最常见的毛病——稿子里已经打了两个全角空格，编辑器再按
/// 中文规则缩进两格，看上去就成了四格。删掉的是"看不见的字符"，正文一个字不少。
pub(crate) fn leading_space(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = content.trim_start_matches(is_stray_space);
        // 整行都是空白的段（空行）不动：那是段落之间的分隔
        if !trimmed.is_empty() && trimmed.len() != content.len() {
            let end = line_start + (content.len() - trimmed.len());
            out.push(Edit::replace("leading_space", line_start, end, ""));
        }
        line_start += line.len();
    }
    out
}

/// 段首算"多余空白"的字符：半角空格、制表符、全角空格、不换行空格。
fn is_stray_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{3000}' | '\u{00A0}')
}

/// 中英之间补一个空格（`中文English` → `中文 English`）。
///
/// **纯风格**：有人这么排，也有人很讨厌这种空格——所以它在"风格"那一档，默认不勾。
pub(crate) fn cjk_latin_space(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    for (i, c) in text.char_indices() {
        if let Some(before) = prev {
            let latin_after_cjk = is_cjk_char(before) && c.is_ascii_alphanumeric();
            let cjk_after_latin = before.is_ascii_alphanumeric() && is_cjk_char(c);
            if latin_after_cjk || cjk_after_latin {
                out.push(Edit::insert("cjk_latin_space", i, " "));
            }
        }
        prev = Some(c);
    }
    out
}

/// 中日韩文字（标点不算——标点后面不加空格）。
fn is_cjk_char(c: char) -> bool {
    text::is_cjk(c) || text::is_kana_or_hangul(c)
}
