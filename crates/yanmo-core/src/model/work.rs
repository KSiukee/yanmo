//! 作品（`works` 表）。

use crate::error::{codes, Error, Result};
use crate::text::WordCaliber;

/// 作品类型——**一等公民**。
///
/// 它决定界面开哪些能力：长篇有卷章树/人物卡/伏笔，文章默认零层级、隐藏这些。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    /// 长篇（连载小说）
    Novel,
    /// 单篇文章 / 短文
    Article,
    /// 短篇集 / 文集
    Collection,
}

impl WorkKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            WorkKind::Novel => "novel",
            WorkKind::Article => "article",
            WorkKind::Collection => "collection",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "novel" => Ok(WorkKind::Novel),
            "article" => Ok(WorkKind::Article),
            "collection" => Ok(WorkKind::Collection),
            other => {
                Err(Error::invalid_with(codes::UNKNOWN_WORK_KIND, [("value", other.to_string())]))
            }
        }
    }

    /// 这个类型**默认**用什么命名规则（作者没在设置里选过时）。
    ///
    /// 长篇天生是"第N章"的写法；单篇与短篇集（散文 / 随笔）里作者都自己起名，
    /// 硬塞一个"第N篇"只会碍事。作者随时可以在设置里改成别的（全局或单本覆盖）。
    pub const fn default_naming(self) -> super::node::NamingStyle {
        match self {
            WorkKind::Novel => super::node::NamingStyle::Arabic,
            WorkKind::Article | WorkKind::Collection => super::node::NamingStyle::NoNumber,
        }
    }

    /// 该类型是否默认使用多层级结构（仅影响 UI 默认，**不影响表结构**）。
    pub const fn default_hierarchical(self) -> bool {
        matches!(self, WorkKind::Novel)
    }
}

/// 作品语言——**作品的属性，跟书走**（与界面语言是两回事）。
///
/// 为什么要有它：字数口径本来就与语言相关（中文逐字 / 英文按词 / 日文原稿纸 400 字），
/// 中文界面的作者写英文小说，字数就该按词算。将来字体与稿纸线型也跟它走。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkLanguage {
    /// 中文
    Zh,
    /// 英文
    En,
    /// 日文
    Ja,
}

impl WorkLanguage {
    pub const fn as_str(self) -> &'static str {
        match self {
            WorkLanguage::Zh => "zh",
            WorkLanguage::En => "en",
            WorkLanguage::Ja => "ja",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "zh" => Ok(WorkLanguage::Zh),
            "en" => Ok(WorkLanguage::En),
            "ja" => Ok(WorkLanguage::Ja),
            other => Err(Error::invalid_with(
                codes::UNKNOWN_WORK_LANGUAGE,
                [("value", other.to_string())],
            )),
        }
    }

    /// 这种语言的**默认字数口径**：中文逐字（含标点，与网文平台口径一致）、
    /// 英文按词、日文逐字。作者在状态栏点一下就能改，这里只是"第一次打开看到哪个"。
    pub const fn default_caliber(self) -> WordCaliber {
        match self {
            WorkLanguage::Zh | WorkLanguage::Ja => WordCaliber::Chars,
            WorkLanguage::En => WordCaliber::Words,
        }
    }
}

/// 一部作品。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub id: i64,
    pub kind: WorkKind,
    pub title: String,
    /// 作品语言（v4 起；老库迁移后一律是中文）。
    pub language: WorkLanguage,
    /// 目标字数（可空＝不设目标）。
    pub target_words: Option<i64>,
    /// 作品简介（v6 起；作者手填，空串＝没写过）。投稿包的大纲要用它。
    pub summary: String,
    /// 创建时间（unix 毫秒）。
    pub created_at: i64,
    /// 最近编辑时间。
    pub updated_at: i64,
    /// 最近打开时间（书架排序用）。
    pub opened_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for k in [WorkKind::Novel, WorkKind::Article, WorkKind::Collection] {
            assert_eq!(WorkKind::parse(k.as_str()).unwrap(), k);
        }
    }

    #[test]
    fn unknown_kind_is_rejected_not_guessed() {
        assert!(WorkKind::parse("poem").is_err());
    }

    #[test]
    fn only_novel_is_hierarchical_by_default() {
        assert!(WorkKind::Novel.default_hierarchical());
        assert!(!WorkKind::Article.default_hierarchical());
        assert!(!WorkKind::Collection.default_hierarchical());
    }
}
