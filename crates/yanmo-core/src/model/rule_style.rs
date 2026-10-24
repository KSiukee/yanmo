//! 正文底下那一层横线用哪种线型：**实线 / 虚线 / 不显示**。
//!
//! 它是**作者的偏好**（存 `appearance`），不是数据的属性：换线型只是把同一份稿子换个底子看，
//! 一个字节都不动，也没有任何后台行为跟着变；界面上改完立刻见效，重启还在。
//!
//! 三档的取舍：稿纸那种"一行一条横线"的写法有人喜欢、有人嫌晃眼，
//! **默认「不显示」**——与研墨一直以来的样子一致，不给没提过这件事的人换底子。
//!
//! 与「行距」的关系只在界面那一侧：一格横线的高度＝正文一行的高度（字号 × 行距），
//! 所以线永远落在每一行底下（见 `editor/rule.ts`）。**核心不存那个高度**——它是排版算出来的，
//! 存一份就会腐烂（作者一改行距，这边还记着上一次）。
//!
//! 取值只有这一处知道：写进来不认识的就当场拒（`value.unknown_rule_style`），
//! 不会给界面埋一个"不知道该画哪种线"的空档。

use crate::error::{codes, Error, Result};

/// 稿纸横线的线型（`appearance.editor_rule_style` 的稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleStyle {
    /// 不画横线（**默认**，与研墨一直以来的样子一致）。
    None,
    /// 实线（一条连着的细横线）。
    Solid,
    /// 虚线（同样一条横线，但断成一截一截的）。
    Dashed,
}

/// 进 JSON 就用**稳定码本身**（与 `Theme` / `SideTab` 同一条纪律：变体名与稳定码是两件事，
/// 靠"改蛇形"迟早分家，而且分家时不报错）。
impl serde::Serialize for RuleStyle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl RuleStyle {
    /// 全部取值（穷举测试拿它对表）。
    pub const ALL: [RuleStyle; 3] = [RuleStyle::None, RuleStyle::Solid, RuleStyle::Dashed];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            RuleStyle::None => "none",
            RuleStyle::Solid => "solid",
            RuleStyle::Dashed => "dashed",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_rule_style`。
    pub fn parse(s: &str) -> Result<Self> {
        RuleStyle::ALL
            .into_iter()
            .find(|style| style.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_RULE_STYLE, [("value", s.to_string())])
            })
    }
}

impl Default for RuleStyle {
    /// 默认**不显示**：不给没提过这件事的人换底子。
    fn default() -> Self {
        RuleStyle::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_style_codes_round_trip_and_reject_strangers() {
        for style in RuleStyle::ALL {
            assert_eq!(RuleStyle::parse(style.as_str()).unwrap(), style);
        }
        assert_eq!(RuleStyle::default(), RuleStyle::None);
        let err = RuleStyle::parse("dotted").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_RULE_STYLE);
    }
}
