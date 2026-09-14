//! 文本度量：字符数、字数、内容指纹、命中片段。
//!
//! # 字数三口径（产品硬承诺，别在别处再算一遍）
//!
//! | 口径 | 代码 | 算什么 |
//! |---|---|---|
//! | 逐字（含标点） | `chars` | 非空白、非零宽的字符全算 |
//! | 逐字（不含标点） | `chars_no_punct` | 只算"字"：汉字 / 假名 / 谚文 / 字母 / 数字 |
//! | 按词 | `words` | CJK（表意·假名·谚文）逐字计 1；其它连续字母数字串计 1（英文按词） |
//!
//! 三个口径都在这一个文件里，[`WordCaliber`](crate::text::WordCaliber) 是它们的唯一入口——**别在别处再写一份**
//! （重复副本＝拆分硬信号，不是收藏品）。口径**跟作品语言走、不跟界面语言走**：
//! 中文界面用户写英文小说，字数就该按词算。
//!
//! 所有下标都是 **Unicode 字符下标**，不是字节下标——中文按字节算会错得离谱。

/// 是否 CJK 表意文字（含扩展 A/兼容区与扩展 B+）。
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x2_0000..=0x2_FA1F)
}

/// 日文假名 / 韩文谚文——**它们也是"逐字"，不是英文串**。
///
/// 为什么要单独认：它们不在表意文字区，却在 Unicode 里带 Alphabetic 属性，
/// 于是会被 [`char::is_alphanumeric`] 当成"字母"，走"英文按词"那条路——
/// 一整句「こんにちは」只算 **1 个词**。日文按原稿纸 400 字计数、韩文按字计数就全错。
pub fn is_kana_or_hangul(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x309F        // 平假名
        | 0x30A0..=0x30FF      // 片假名
        | 0x31F0..=0x31FF      // 片假名语音扩展
        | 0xAC00..=0xD7AF      // 谚文音节
        | 0x1100..=0x11FF      // 谚文字母
        | 0x3130..=0x318F      // 谚文兼容字母
        | 0xFF66..=0xFF9D)     // 半角片假名
}

/// 零宽字符：占位置但**看不见、也不是作者写的字**（零宽空格/连字、方向标记、BOM）。
///
/// 它们不是空白（`is_whitespace` 不认），所以必须单独排掉——否则从网页复制的正文里
/// 混进来的零宽字符会把字数撑大，而且作者怎么数都对不上。
fn is_zero_width(c: char) -> bool {
    matches!(c as u32, 0x200B..=0x200F | 0x2060..=0x2064 | 0xFEFF)
}

/// 算不算"字"（不含标点口径的判据）：汉字 / 假名 / 谚文 / 字母 / 数字。
///
/// 全角字母数字（`Ａ１２`）在 Unicode 里本就是字母数字，自动算进来；
/// 标点、符号、emoji 一律不算。
fn is_word_char(c: char) -> bool {
    is_cjk(c) || is_kana_or_hangul(c) || c.is_alphanumeric()
}

/// 逐字口径：**非空白、非零宽**的字符全算（标点、emoji 也算——它们各占一个字位）。
pub fn count_chars(text: &str) -> i64 {
    text.chars().filter(|c| !c.is_whitespace() && !is_zero_width(*c)).count() as i64
}

/// 逐字（不含标点）口径：只算 `is_word_char`。
pub fn count_chars_no_punct(text: &str) -> i64 {
    text.chars().filter(|c| is_word_char(*c)).count() as i64
}

/// 按词口径：CJK（表意·假名·谚文）逐字计 1；其它连续字母/数字串计 1（英文按词）；标点不计。
pub fn count_words(text: &str) -> i64 {
    let mut words = 0i64;
    let mut in_run = false;
    for c in text.chars() {
        if is_cjk(c) || is_kana_or_hangul(c) {
            words += 1;
            in_run = false;
        } else if c.is_alphanumeric() {
            if !in_run {
                words += 1;
                in_run = true;
            }
        } else {
            in_run = false;
        }
    }
    words
}

/// 字数口径：**存进设置、进 JSON，值必须稳定**（改值 = 改数据契约）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCaliber {
    /// 逐字（含标点）
    Chars,
    /// 逐字（不含标点）
    CharsNoPunct,
    /// 按词（CJK 逐字 + 英文按词）
    Words,
}

impl WordCaliber {
    /// 全部口径，**按界面循环点击的顺序**（唯一一份顺序，界面别自己排）。
    pub const ALL: [WordCaliber; 3] = [WordCaliber::Chars, WordCaliber::CharsNoPunct, WordCaliber::Words];

    /// 稳定代码（设置 / JSON 里存的就是它；界面文案在字典里，不在核心）。
    pub const fn as_str(self) -> &'static str {
        match self {
            WordCaliber::Chars => "chars",
            WordCaliber::CharsNoPunct => "chars_no_punct",
            WordCaliber::Words => "words",
        }
    }

    /// 认代码；认不出来返回 `None`（设置里的坏记录当没设过，由调用方回默认）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "chars" => Some(WordCaliber::Chars),
            "chars_no_punct" => Some(WordCaliber::CharsNoPunct),
            "words" => Some(WordCaliber::Words),
            _ => None,
        }
    }

    /// 数一遍。
    pub fn count(self, text: &str) -> i64 {
        match self {
            WordCaliber::Chars => count_chars(text),
            WordCaliber::CharsNoPunct => count_chars_no_punct(text),
            WordCaliber::Words => count_words(text),
        }
    }
}

/// 过界面时序列化成**稳定代码**（不是 Rust 变体名）——界面与设置看到的是同一套值。
impl serde::Serialize for WordCaliber {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 内容指纹：FNV-1a 64 位，16 位十六进制。
///
/// 用途是**变更检测**（"这一版和库里那版是不是同一份"），**不是密码学哈希**——
/// 别拿它做完整性证明或安全校验。
pub fn content_hash(text: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

/// 单字符比较：ASCII 大小写不敏感（中文本来就无大小写）。
fn same_char(a: char, b: char) -> bool {
    a == b || (a.is_ascii() && b.is_ascii() && a.eq_ignore_ascii_case(&b))
}

/// 在文本里找 `needle`，返回**字符下标**；找不到返回 `None`。
///
/// 复杂度 O(n·m)，只在已命中的少量行上做高亮用，不做全库扫描。
pub fn find_char_index(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<char> = haystack.chars().collect();
    let pat: Vec<char> = needle.chars().collect();
    if pat.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - pat.len()).find(|&i| pat.iter().enumerate().all(|(j, p)| same_char(hay[i + j], *p)))
}

/// 命中片段：以首个命中为中心取 `context` 字上下文，两端被截断处加省略号。
///
/// 换行折成空格——片段是给列表显示的一行，不是正文。
pub fn snippet(body: &str, needle: &str, context: usize) -> Option<String> {
    let at = find_char_index(body, needle)?;
    let chars: Vec<char> = body.chars().collect();
    let start = at.saturating_sub(context);
    let end = (at + needle.chars().count() + context).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(chars[start..end].iter().map(|c| if *c == '\n' || *c == '\r' { ' ' } else { *c }));
    if end < chars.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars_ignore_whitespace_and_zero_width() {
        assert_eq!(count_chars("你好，世界"), 5);
        assert_eq!(count_chars("你好 世界\n第二行"), 7);
        // 从网页复制来的正文常混零宽字符：看不见，也不该算成字
        assert_eq!(count_chars("你\u{200b}好\u{feff}世界"), 4);
    }

    #[test]
    fn chars_no_punct_counts_only_letters_digits_and_ideographs() {
        assert_eq!(count_chars_no_punct("你好，世界"), 4, "中文标点不算");
        assert_eq!(count_chars_no_punct("hello world"), 10, "这一口径英文按字母");
        assert_eq!(count_chars_no_punct("……——「」"), 0);
        assert_eq!(count_chars_no_punct("Ａ１２"), 3, "全角字母数字也是字");
        assert_eq!(count_chars_no_punct("你好🙂世界"), 4, "emoji 不是字");
    }

    #[test]
    fn words_are_cjk_per_char_and_ascii_per_run() {
        assert_eq!(count_words("你好，世界"), 4);
        assert_eq!(count_words("hello world"), 2);
        // 第 / 1 / 章 / Rust / 入 / 门
        assert_eq!(count_words("第1章 Rust 入门"), 6);
    }

    #[test]
    fn kana_and_hangul_count_per_char_not_as_one_word() {
        // 它们带 Alphabetic 属性，不单独认就会被当成"一个英文串"
        assert_eq!(count_words("こんにちは"), 5);
        assert_eq!(count_words("안녕하세요"), 5);
        assert_eq!(count_chars_no_punct("こんにちは"), 5);
        assert_eq!(count_words("これは Rust です"), 6, "假名逐字（3+2）、Rust 按词：5 + 1");
    }

    #[test]
    fn caliber_codes_round_trip_and_dispatch() {
        let text = "你好，world";
        assert_eq!(WordCaliber::Chars.count(text), 8); // 你好，world = 7 字 + 逗号
        assert_eq!(WordCaliber::CharsNoPunct.count(text), 7);
        assert_eq!(WordCaliber::Words.count(text), 3); // 你 / 好 / world
        for caliber in WordCaliber::ALL {
            assert_eq!(WordCaliber::parse(caliber.as_str()), Some(caliber), "代码要能转回来");
        }
        assert_eq!(WordCaliber::parse("nonsense"), None);
        assert_eq!(WordCaliber::ALL[0], WordCaliber::Chars, "循环顺序的第一档是逐字（含标点）");
    }

    #[test]
    fn hash_detects_change_and_is_stable() {
        assert_eq!(content_hash("第一章"), content_hash("第一章"));
        assert_ne!(content_hash("第一章"), content_hash("第一章 "));
        assert_eq!(content_hash("").len(), 16);
    }

    #[test]
    fn find_is_case_insensitive_for_ascii_and_char_indexed() {
        assert_eq!(find_char_index("他说：Rust 很好", "rust"), Some(3));
        assert_eq!(find_char_index("abcdef", "cd"), Some(2));
        assert_eq!(find_char_index("短", "很长很长"), None);
        assert_eq!(find_char_index("任意", ""), None);
    }

    #[test]
    fn snippet_marks_truncation_and_flattens_newlines() {
        let body = "前面很多字。目标句在这里。后面还有很多字。";
        let s = snippet(body, "目标", 3).unwrap();
        assert!(s.starts_with('…') && s.ends_with('…'), "两端截断应有省略号：{s}");
        assert!(s.contains("目标"));
        let s2 = snippet("第一行\n目标", "目标", 5).unwrap();
        assert!(!s2.contains('\n'), "片段里不应有换行：{s2:?}");
    }
}
