//! 正文背景那一族的**稳定码与可接受范围**：底色、底纹、图片层的不透明度。
//!
//! 它们与「稿纸横线」（`model::rule_style`）是同一族东西——**纯显示层**：只改作者看正文的
//! 观感，不进导出、不动正文一个字节。分成两件是因为变化理由不同：
//! 线型只有三个值，而背景这一族有底色 / 底纹 / 图片三层，各自还有自己的范围。
//!
//! # 三条口径
//!
//! 1. **码进库、中文名住界面字典**（核心零文案）：库里存 `paper` / `grid` 这样的稳定码，
//!    界面上显示成什么由字典说了算。
//! 2. **认不出来的码当没设过**：读的时候回默认（与口径 / 引号 / 配色同一条规矩），
//!    写的时候才当场拒——坏值不该把界面卡住，但也不该被悄悄收下。
//! 3. **`auto` = 清掉这一层**：界面要能表达"这一项不要单独设了"（回继承 / 回默认），
//!    而"不传"表达的是"这项不改"——两者不是一回事（与命名规则、配色同一条路）。
//!
//! # 图片那一格只存"文件叫什么"
//!
//! **不存路径**：图片由壳在作者选图那一步复制进数据目录，库里只留一个文件名
//! （见 `store::appearance` 的 `editor_bg_image`）。界面拿不到任何路径，也就无从拼出
//! 一个数据目录之外的路径来。

/// 正文底色（**浅色那一套**；深色配色下由界面按 `data-theme` 换一组底）。
///
/// 它只改编辑区那一层的底色，**不动段落与文字**，也不进导出。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundColor {
    /// 纸白——与研墨一直以来的编辑区底色一致（默认）
    #[default]
    Paper,
    /// 米黄（护眼纸）
    Cream,
    /// 淡青（豆绿纸）
    Green,
    /// 浅灰（冷调纸）
    Gray,
}

/// 进 JSON 就用**稳定码本身**（与 `Theme` / `RuleStyle` 同一条纪律：变体名与稳定码是两件事，
/// 靠"改蛇形"迟早分家，而且分家时不报错）。
impl serde::Serialize for BackgroundColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl BackgroundColor {
    /// 稳定代码（写进库与设置；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            BackgroundColor::Paper => "paper",
            BackgroundColor::Cream => "cream",
            BackgroundColor::Green => "green",
            BackgroundColor::Gray => "gray",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_background_color`。
    pub fn parse(s: &str) -> crate::error::Result<Self> {
        match s {
            "paper" => Ok(BackgroundColor::Paper),
            "cream" => Ok(BackgroundColor::Cream),
            "green" => Ok(BackgroundColor::Green),
            "gray" => Ok(BackgroundColor::Gray),
            other => Err(crate::error::Error::invalid_with(
                crate::error::codes::UNKNOWN_BACKGROUND_COLOR,
                [("value", other.to_string())],
            )),
        }
    }

    /// 界面穷举用的那一串（顺序就是设置面板里摆的先后）。
    pub const ALL: [BackgroundColor; 4] = [
        BackgroundColor::Paper,
        BackgroundColor::Cream,
        BackgroundColor::Green,
        BackgroundColor::Gray,
    ];
}

/// 正文底纹——**垫在字底下的一层纹路**（与「稿纸横线」是同一族，但各画各的）。
///
/// 为什么不做成"纹路 + 线型"两个维度：方格自己就含横线与竖线，再把横线那一档叠上去
/// 只会打架。作者要横线就选「稿纸横线」那一组（排版），要方格就在这儿选方格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundLayout {
    /// 不铺（默认）——与研墨一直以来的样子一致
    #[default]
    None,
    /// 方格（横竖都画，一格 = 字号 × 行距）
    Grid,
    /// 点阵（一格一个淡淡的点）
    Dots,
    /// 旧纸（斜向的细纹，像放了很久的纸）
    Grain,
}

/// 进 JSON 就用**稳定码本身**（理由同上）。
impl serde::Serialize for BackgroundLayout {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl BackgroundLayout {
    /// 稳定代码（写进库；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            BackgroundLayout::None => "none",
            BackgroundLayout::Grid => "grid",
            BackgroundLayout::Dots => "dots",
            BackgroundLayout::Grain => "grain",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_background_layout`。
    pub fn parse(s: &str) -> crate::error::Result<Self> {
        match s {
            "none" => Ok(BackgroundLayout::None),
            "grid" => Ok(BackgroundLayout::Grid),
            "dots" => Ok(BackgroundLayout::Dots),
            "grain" => Ok(BackgroundLayout::Grain),
            other => Err(crate::error::Error::invalid_with(
                crate::error::codes::UNKNOWN_BACKGROUND_LAYOUT,
                [("value", other.to_string())],
            )),
        }
    }

    /// 界面穷举用的那一串（顺序就是设置面板里摆的先后）。
    pub const ALL: [BackgroundLayout; 4] = [
        BackgroundLayout::None,
        BackgroundLayout::Grid,
        BackgroundLayout::Dots,
        BackgroundLayout::Grain,
    ];
}

/// 图片铺法：铺不满时怎么办。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundFit {
    /// 铺满（裁掉多出来的边）——默认：看着最像"这张纸"
    #[default]
    Cover,
    /// 装下（整张都看得见，边上留底色）
    Contain,
    /// 平铺（原尺寸一块一块接起来，适合纹理图）
    Tile,
}

/// 进 JSON 就用**稳定码本身**（理由同上）。
impl serde::Serialize for BackgroundFit {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl BackgroundFit {
    /// 稳定代码（写进库；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            BackgroundFit::Cover => "cover",
            BackgroundFit::Contain => "contain",
            BackgroundFit::Tile => "tile",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_background_fit`。
    pub fn parse(s: &str) -> crate::error::Result<Self> {
        match s {
            "cover" => Ok(BackgroundFit::Cover),
            "contain" => Ok(BackgroundFit::Contain),
            "tile" => Ok(BackgroundFit::Tile),
            other => Err(crate::error::Error::invalid_with(
                crate::error::codes::UNKNOWN_BACKGROUND_FIT,
                [("value", other.to_string())],
            )),
        }
    }

    /// 界面穷举用的那一串（顺序就是设置面板里摆的先后）。
    pub const ALL: [BackgroundFit; 3] =
        [BackgroundFit::Cover, BackgroundFit::Contain, BackgroundFit::Tile];
}

/// 图片层不透明度的默认值与上限（百分比）。
///
/// 默认 **35%**：图片是"垫一层气氛"，不该盖过字——想更明显可以自己往上拉，
/// 拉到 100 也照样让拉（可读性只提示不硬挡，见界面那一栏的提示语）。
pub const DEFAULT_BG_OPACITY: i64 = 35;
pub const MAX_BG_OPACITY: i64 = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_roundtrips() {
        for color in BackgroundColor::ALL {
            assert_eq!(BackgroundColor::parse(color.as_str()).unwrap(), color);
        }
        for layout in BackgroundLayout::ALL {
            assert_eq!(BackgroundLayout::parse(layout.as_str()).unwrap(), layout);
        }
        for fit in BackgroundFit::ALL {
            assert_eq!(BackgroundFit::parse(fit.as_str()).unwrap(), fit);
        }
    }

    /// 默认必须是"与研墨一直以来的样子一模一样"：纸白 + 不铺。
    #[test]
    fn defaults_keep_the_old_look() {
        assert_eq!(BackgroundColor::default(), BackgroundColor::Paper);
        assert_eq!(BackgroundLayout::default(), BackgroundLayout::None);
        assert_eq!(BackgroundFit::default(), BackgroundFit::Cover);
        assert_eq!(DEFAULT_BG_OPACITY, 35, "默认不透明度是「垫一层气氛」那一档");
    }

    #[test]
    fn unknown_codes_are_rejected_not_guessed() {
        assert!(BackgroundColor::parse("sepia").is_err());
        assert!(BackgroundLayout::parse("lined").is_err(), "横线归稿纸线型那一族，不在这里");
        assert!(BackgroundFit::parse("stretch").is_err());
    }

    /// 穷举表不许漏项也不许重复（界面按它摆选项，漏一个作者就选不到）。
    #[test]
    fn all_lists_are_complete_and_unique() {
        let colors: Vec<&str> = BackgroundColor::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(colors, vec!["paper", "cream", "green", "gray"]);
        let layouts: Vec<&str> = BackgroundLayout::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(layouts, vec!["none", "grid", "dots", "grain"]);
        let fits: Vec<&str> = BackgroundFit::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(fits, vec!["cover", "contain", "tile"]);
    }
}
