//! 手滑类规则：衍字（同一个字连打两遍）与重复词（两字词连打两遍）。
//!
//! 中文里"叠"大多是**正常修辞**：看看、慢慢、一个个、高高兴兴、研究研究……
//! 所以这一条**只认那些绝不叠着用的字与词**——宁可漏报，也绝不改掉作者的修辞。
//!
//! 判据用**黑名单**而不是白名单：合法叠用列不完（白名单永远漏），但"从来不会叠用"的字词
//! 是可以穷举的（虚词、代词、连词那些），列进去的每一条都经得起推敲。

use super::{char_at, run_end, Edit};

// i18n-allow-begin: 下面两张表是**判据数据**（哪些字、哪些词绝不叠着用），不是界面文案；
// 界面只认规则码（repeat_char / repeat_word），名字在 locales 里
/// 绝不叠用的单字：连打两遍就是手滑（`的的` / `他他` / `很很`）。
///
/// 只放**虚词、代词、连词、副词**这一类——它们没有叠用的用法，
/// 而"看看/慢慢/好好/个个"这些能叠的字一个都不在这里。
const NEVER_DOUBLED: &str = "的地得了她它你我他是和就都也把被这那很又再却从向为而或及们吧呢与在";

/// 绝不叠用的两字词（虚词类）：`已经已经` / `因为因为` / `的时候` 打两遍。
///
/// 同样只放没有叠用用法的词——"研究研究""商量商量""漂亮漂亮"这类合法的动词/形容词重叠
/// 一个都不收，所以不会误伤。
const NEVER_REPEATED: &[&str] = &[
    "已经", "因为", "但是", "所以", "如果", "虽然", "而且", "然后", "于是", "因此",
    "可是", "并且", "或者", "以及", "不过", "尽管", "即使", "无论", "除非", "然而",
];
// i18n-allow-end

/// 衍字：黑名单里的字连打两遍（三遍也一起并掉）。
pub(crate) fn repeat_char(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let c = char_at(text, i);
        let end = run_end(text, i, c);
        if end - i > c.len_utf8() && NEVER_DOUBLED.contains(c) {
            out.push(Edit::replace("repeat_char", i, end, &c.to_string()));
        }
        i = end;
    }
    out
}

/// 重复词：黑名单里的两字词连着打了两遍以上（三遍也一起并掉）。
pub(crate) fn repeat_word(text: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    for word in NEVER_REPEATED {
        let len = word.len();
        for (at, _) in text.match_indices(word) {
            if !text[at + len..].starts_with(word) {
                continue;
            }
            let mut end = at + len;
            while text[end..].starts_with(word) {
                end += len;
            }
            // 同一个词连着出现时，`match_indices` 会把每一遍都报一次：只留第一处
            if out.last().is_some_and(|last: &Edit| last.start < end && at < last.end) {
                continue;
            }
            out.push(Edit::replace("repeat_word", at, end, word));
        }
    }
    out.sort_by_key(|edit| (edit.start, edit.end));
    out
}
