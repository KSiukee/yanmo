//! 界面用哪一套配色：**浅色 / 深色 / 跟随系统**。
//!
//! 它是**作者的偏好**（存 `appearance`，跟人不跟书），不是数据的属性：
//! 换一套配色只是把同一份稿子换个底子看，一个字节都不动，也没有任何后台行为跟着变。
//!
//! 三档的取舍：作者白天可能在浅色里写、夜里换深色；"跟随系统"是给"不想每次自己切"的人
//! 留的一档（跟着操作系统的深浅色走）。**默认浅色**——与研墨一直以来的样子一致，
//! 不给没提过这件事的人换个底子（暗色是他自己挑的，不是我们替他挑的）。
//!
//! 为什么不在这里存"跟随系统时系统到底是深是浅"：那是**界面当场能问到的**（`prefers-color-scheme`），
//! 核心存一份就会腐烂——系统换了、这边还记着上一次。核心只管"作者选的是哪一档"。
//!
//! 取值只有这一处知道：写进来不认识的就当场拒（`value.unknown_theme`），
//! 不会给界面埋一个"不知道该画哪套"的空档。

use crate::error::{codes, Error, Result};

/// 界面配色（`appearance.theme` 的稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// 浅色（**默认**，与研墨一直以来的样子一致）。
    Light,
    /// 深色（夜里码字）。
    Dark,
    /// 跟随系统（操作系统深色就深色）。
    System,
}

/// 进 JSON 就用**稳定码本身**（与 `SideTab` 同一条纪律：变体名与稳定码是两件事，
/// 靠"改蛇形"迟早分家，而且分家时不报错）。
impl serde::Serialize for Theme {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl Theme {
    /// 全部取值（穷举测试拿它对表）。
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::System];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_theme`。
    pub fn parse(s: &str) -> Result<Self> {
        Theme::ALL
            .into_iter()
            .find(|theme| theme.as_str() == s)
            .ok_or_else(|| Error::invalid_with(codes::UNKNOWN_THEME, [("value", s.to_string())]))
    }
}

impl Default for Theme {
    /// 默认**浅色**：不给没提过这件事的人换底子。
    fn default() -> Self {
        Theme::Light
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_codes_round_trip_and_reject_strangers() {
        for theme in Theme::ALL {
            assert_eq!(Theme::parse(theme.as_str()).unwrap(), theme);
        }
        assert_eq!(Theme::default(), Theme::Light);
        let err = Theme::parse("sepia").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_THEME);
    }
}
