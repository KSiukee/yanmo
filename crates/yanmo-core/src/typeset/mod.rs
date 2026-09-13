//! 中文排版规范化：**只报告、可逐条确认**的正文清理。
//!
//! 四条要一直守住的规矩：
//!
//! 1. **扫描与应用分家**：`scan` 只回答「哪里会怎么改」，`apply` 只改作者真的勾中的那几条。
//!    **没有"一键静默改全篇"的入口**——正文是作者的原稿，误改标点比不改更贵。
//! 2. **能改的与只能提醒的分开**：有确定改法的进 `changes`（逐条勾选）；
//!    没有确定改法的（引号括号缺一半）进 `notices`——**只报告、不猜着补**。
//! 3. **核心不产出界面句子**：每条规则只有一个稳定代码（`ellipsis` / `quote`…），
//!    名字与说明由界面对着自己的字典取（与错误码同一条纪律）。
//! 4. **幂等**：应用过一遍之后再扫，命中必须是 0——同一处不该被反复改来改去。
//!
//! 它只读一段文本、回一段文本：不碰数据模型、不进导出、不知道稿子存在哪。
//!
//! 每条规则还带一个**风险档**（`safe` / `careful` / `style`）：界面据此决定默认勾哪几条。
//! 档位只写在这里一处——界面不抄一份，免得两边走偏。

pub mod report;
pub mod rules;

use serde::{Deserialize, Serialize};

use crate::error::codes;
use crate::error::{Error, Result};

pub use report::{apply, scan, Change, Notice, Report};

/// 风险档：默认勾选与否只看它（界面不自己判断哪条"安全"）。
pub const TIER_SAFE: &str = "safe";
/// 会动到文风（连着打的标点、作者习惯的半角标点），要作者自己勾。
pub const TIER_CAREFUL: &str = "careful";
/// 纯风格偏好（中英之间加不加空格），默认绝不勾。
pub const TIER_STYLE: &str = "style";

/// 一条规则：稳定代码 + 风险档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RuleInfo {
    pub code: &'static str,
    pub tier: &'static str,
}

/// 规则清单（**顺序就是界面上的顺序**，也是同位置冲突时的优先级）。
pub const RULES: &[RuleInfo] = &[
    RuleInfo { code: "ellipsis", tier: TIER_SAFE },
    RuleInfo { code: "dash", tier: TIER_SAFE },
    RuleInfo { code: "leading_space", tier: TIER_SAFE },
    // 衍字看着像手滑，但对话里的结巴（"你你你说什么"）是修辞，所以归"自己勾"那一档
    RuleInfo { code: "repeat_char", tier: TIER_CAREFUL },
    RuleInfo { code: "repeat_word", tier: TIER_CAREFUL },
    RuleInfo { code: "repeat_punct", tier: TIER_CAREFUL },
    RuleInfo { code: "halfwidth_punct", tier: TIER_CAREFUL },
    RuleInfo { code: "quote", tier: TIER_CAREFUL },
    RuleInfo { code: "cjk_latin_space", tier: TIER_STYLE },
];

/// **只报告、不给改法**的检查（认在 [`Notice`] 里）：缺一半的引号该补在哪儿只有作者知道。
pub const NOTICE_RULES: &[&str] = &["pair_missing"];

/// 引号的两套偏好：大陆通用的弯引号，或台港常用的角引号。
///
/// 序列化出来就是那两个稳定代码（`curly` / `corner`）——界面拿它当取值用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuoteStyle {
    /// `“”` / `‘’`
    Curly,
    /// `「」` / `『』`
    Corner,
}

impl QuoteStyle {
    /// 认不出来就是没设过（与别的偏好同一规矩：坏值不许装成好值）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "curly" => Some(Self::Curly),
            "corner" => Some(Self::Corner),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Curly => "curly",
            Self::Corner => "corner",
        }
    }
}

impl Default for QuoteStyle {
    fn default() -> Self {
        Self::Curly
    }
}

/// 扫描时要用的选项（目前只有引号偏好一种）。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Options {
    /// `curly`（默认）/ `corner`；不传就是默认
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_style: Option<String>,
}

impl Options {
    /// 落定引号偏好；传了一个不认识的风格要报错，不许悄悄按默认走。
    pub fn quote_style(&self) -> Result<QuoteStyle> {
        match self.quote_style.as_deref() {
            None => Ok(QuoteStyle::default()),
            Some(value) => QuoteStyle::parse(value).ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_QUOTE_STYLE, [("value", value.to_string())])
            }),
        }
    }
}
