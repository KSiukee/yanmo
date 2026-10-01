//! 「这一章在推进哪几条线」的**固定五类**：主线 / 副线 / 感情线 / 悬念线 / 成长线。
//!
//! 它是一个**闭集**：五条就是五条，不设"自定义线名"——自定义线名一开，
//! "哪几条线在推进"就退化成作者自己随手打的标签，统计与叩问都拿它做不了判据。
//! 要新加一类就改这张表的版本，而不是让每一本书各有一套。
//!
//! # 三条口径（写在这里，省得每处调用各猜一套）
//!
//! 1. **一章可以同时推进多条**，也可以一条都不标；
//! 2. **"没标"不等于"没推进"**——空是这个字段的常态（老库里的章一次都没标过），
//!    所以读出来是空数组时，界面**不许**替作者说成"这一章没推进任何线"；
//! 3. **核心只认码**（`main` / `sub` / `romance` / `suspense` / `growth`）：
//!    中文名（"主线""副线"…）住在界面字典里，核心不出中文（`tools/qa/scan_text.py` 盯着这条）。
//!
//! 码**稳定、别改**：它进库（`node_lines.line`），改了就等于把作者的标注读成别人的意思。
//! 与 [`SideTab`](crate::model::SideTab) / [`QuestionTone`](crate::model::QuestionTone) 同一个路数：
//! 进 JSON 就是码本身，不靠"改蛇形"派生（派生分家时不报错，只是界面静默对不上）。

use crate::error::{codes, Error, Result};

/// 故事线（`node_lines.line` 的稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoryLine {
    /// 主线：这本书的主干往前走。
    Main,
    /// 副线：与主干并行的那一条。
    Sub,
    /// 感情线。
    Romance,
    /// 悬念线：吊着读者的那一头。
    Suspense,
    /// 成长线：人物自己变了多少。
    Growth,
}

/// 进 JSON 就用**稳定码本身**（进库那一列同理，见 [`StoryLine::as_str`]）。
impl serde::Serialize for StoryLine {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl StoryLine {
    /// 全部取值（**界面上那一排的顺序**；穷举测试拿它对表，比对返回的稳定序也按它排）。
    pub const ALL: [StoryLine; 5] = [
        StoryLine::Main,
        StoryLine::Sub,
        StoryLine::Romance,
        StoryLine::Suspense,
        StoryLine::Growth,
    ];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            StoryLine::Main => "main",
            StoryLine::Sub => "sub",
            StoryLine::Romance => "romance",
            StoryLine::Suspense => "suspense",
            StoryLine::Growth => "growth",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_story_line`。
    ///
    /// **不静默回退**（比如猜成主线、或当成"没标"）：入库的口径只有一个，
    /// 猜错一次就会把作者的标注记到别的线上，而那是**看不出来**的一类错。
    pub fn parse(text: &str) -> Result<Self> {
        StoryLine::ALL
            .into_iter()
            .find(|line| line.as_str() == text)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_STORY_LINE, [("value", text.to_string())])
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 码的往返 + `ALL` 穷举（不重不漏：五个码都有人在，且没有两个变体共用一个码）。
    #[test]
    fn story_line_codes_round_trip_and_cover_every_variant() {
        assert_eq!(StoryLine::ALL.len(), 5, "五条线就是五条");
        for (index, line) in StoryLine::ALL.into_iter().enumerate() {
            assert_eq!(StoryLine::parse(line.as_str()).unwrap(), line, "码要能走一个来回");
            assert!(
                !StoryLine::ALL[..index].iter().any(|earlier| earlier.as_str() == line.as_str()),
                "码 {} 被两个变体共用——读回来时就分不清是哪一条了",
                line.as_str()
            );
        }
    }

    /// 认不出的码当场报错，错误里带得出那五个码（界面/命令行据此说清"只认哪些"）。
    #[test]
    fn unknown_codes_are_rejected_with_the_five_codes_in_the_error() {
        let err = StoryLine::parse("plot").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_STORY_LINE);
        let shown = err.to_string();
        for line in StoryLine::ALL {
            assert!(shown.contains(line.as_str()), "错误里该带上 {line:?} 的码：{shown}");
        }
        // 大小写不认、空白不认（码是码，不是让作者输的东西）
        assert!(StoryLine::parse("Main").is_err());
        assert!(StoryLine::parse(" main").is_err());
    }

    /// 进 JSON 是**码本身**（不是变体名、不是序号）。
    #[test]
    fn json_is_the_stable_code() {
        let text = serde_json::to_string(&StoryLine::ALL.to_vec()).unwrap();
        assert_eq!(text, r#"["main","sub","romance","suspense","growth"]"#);
    }
}
