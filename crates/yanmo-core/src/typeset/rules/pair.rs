//! 配对检查：引号、括号、书名号**缺一半**。
//!
//! **只报告，不改稿**：少了的那个右引号到底该补在哪儿、要不要补，只有作者知道——
//! 猜着补就是替作者改稿（"只报告不自动改稿"那条纪律）。所以它进"提醒"，不进"改哪几处"。

/// 一处配对问题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sighting {
    /// 符号对的稳定代码（`quote_double` / `corner` / `paren` / `title` / `bracket`）
    pub mark: &'static str,
    /// `unclosed`＝开着的没关（少一个右边）；`unopened`＝关的那个没有开（多一个右边）
    pub side: &'static str,
    /// 出问题的那个符号在文中的字节位置（报告层拿它算段落与上下文）
    pub at: usize,
}

/// 符号对：代码 + 左 + 右。
const PAIRS: &[(&str, char, char)] = &[
    ("quote_double", '“', '”'),
    ("quote_single", '‘', '’'),
    ("corner", '「', '」'),
    ("corner_single", '『', '』'),
    ("paren", '（', '）'),
    ("title", '《', '》'),
    ("bracket", '【', '】'),
];

/// 一段一段地数：哪一段里某一对符号的开合数目不一样，就在那一段报一处。
///
/// 为什么不整篇一起数：符号极少跨段（真跨段的多半也是漏了），按段报作者才找得到。
pub(crate) fn pair_missing(text: &str) -> Vec<Sighting> {
    let mut out = Vec::new();
    let mut para_start = 0;
    for paragraph in text.split("\n\n") {
        out.extend(paragraph_sightings(paragraph, para_start));
        para_start += paragraph.len() + 2; // "\n\n" 正好两个字节
    }
    out
}

fn paragraph_sightings(paragraph: &str, offset: usize) -> Vec<Sighting> {
    let mut found = Vec::new();
    for (mark, open, close) in PAIRS {
        let opens = paragraph.matches(*open).count();
        let closes = paragraph.matches(*close).count();
        if opens == closes {
            continue;
        }
        // 哪边多，就是哪边没配上；报告多出来的那一只最后一次出现的位置
        let (side, target) = if opens > closes { ("unclosed", *open) } else { ("unopened", *close) };
        if let Some(at) = paragraph.rfind(target) {
            found.push(Sighting { mark, side, at: offset + at });
        }
    }
    found
}
