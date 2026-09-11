//! 文本度量：字符数、字数、内容指纹、命中片段。
//!
//! **口径归属**：中文计数最终要做成「逐字 / 不含标点 / 英文按词」三口径可切换，
//! 那是后续「中文字数统计」任务的活；本模块是它的**落点**——先给最小可用口径，
//! 以后在同一处扩展，不另起第二份实现（重复副本是拆分硬信号，不是收藏品）。
//!
//! 所有下标都是 **Unicode 字符下标**，不是字节下标——中文按字节算会错得离谱。

/// 是否 CJK 表意文字（含扩展 A/兼容区与扩展 B+）。
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x2_0000..=0x2_FA1F)
}

/// 字符数：**不计空白与换行**（空格、制表、换行都不算一个字）。
pub fn count_chars(text: &str) -> i64 {
    text.chars().filter(|c| !c.is_whitespace()).count() as i64
}

/// 字数：CJK 逐字计 1；非 CJK 的连续字母/数字串计 1（英文按词）；标点不计。
pub fn count_words(text: &str) -> i64 {
    let mut words = 0i64;
    let mut in_run = false;
    for c in text.chars() {
        if is_cjk(c) {
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
    fn chars_ignore_whitespace_only() {
        assert_eq!(count_chars("你好，世界"), 5);
        assert_eq!(count_chars("你好 世界\n第二行"), 7);
    }

    #[test]
    fn words_are_cjk_per_char_and_ascii_per_run() {
        assert_eq!(count_words("你好，世界"), 4);
        assert_eq!(count_words("hello world"), 2);
        // 第 / 1 / 章 / Rust / 入 / 门
        assert_eq!(count_words("第1章 Rust 入门"), 6);
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
