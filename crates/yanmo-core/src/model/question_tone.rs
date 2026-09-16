//! 叩问问话的**语气**：温柔 / 中性 / 直接。
//!
//! 它只改「**怎么问**」，绝不改「**问什么**」——三档背后是同一套模板与同一个引力公式，
//! 差别只在界面字典里那句句子挑哪一版（`question.template.<键>` / `.warm` / `.direct`）。
//!
//! **中性＝关**：选它就是不加语气（与最早的写法一样）。所以不必另设一个总开关——
//! 两个开关只会互相打架。
//!
//! 两条护栏（写在这里，也好在评审时对着看）：
//!
//! 1. **不说教**：三档都不许出现"你应该""你必须"这类口气——叩问是陪你捋，不是教你写；
//! 2. **不评价剧情**：绝不评"这样写好/不好"，只问事实与打算。
//!
//! 语气是**作者的偏好**（存 `appearance`，全局打底 + 每书覆盖），不是数据的属性：
//! 换一档，只是同一句话换个说法，一个字节的稿子都不动。

use crate::error::{codes, Error, Result};

/// 问话语气（`appearance.question_tone` 的稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum QuestionTone {
    /// 温柔：先接住、再问（默认那档）。
    Warm,
    /// 中性：不加语气——**这一档就是"关"**。
    Neutral,
    /// 直接：不绕弯子，一句问到底。
    Direct,
}

impl QuestionTone {
    /// 全部取值（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [QuestionTone; 3] = [QuestionTone::Warm, QuestionTone::Neutral, QuestionTone::Direct];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            QuestionTone::Warm => "warm",
            QuestionTone::Neutral => "neutral",
            QuestionTone::Direct => "direct",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_question_tone`。
    pub fn parse(s: &str) -> Result<Self> {
        QuestionTone::ALL
            .into_iter()
            .find(|tone| tone.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_QUESTION_TONE, [("value", s.to_string())])
            })
    }
}

impl Default for QuestionTone {
    /// 默认**温柔**：叩问是坐在旁边陪你捋的那个人，不是审稿的。
    fn default() -> Self {
        QuestionTone::Warm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_codes_round_trip_and_reject_strangers() {
        for tone in QuestionTone::ALL {
            assert_eq!(QuestionTone::parse(tone.as_str()).unwrap(), tone);
        }
        assert_eq!(QuestionTone::default(), QuestionTone::Warm);
        let err = QuestionTone::parse("shouty").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_QUESTION_TONE);
    }
}
