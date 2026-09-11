//! 作品（`works` 表）。

use crate::error::{Error, Result};

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
            other => Err(Error::Invalid(format!("未知的作品类型：{other}"))),
        }
    }

    /// 该类型是否默认使用多层级结构（仅影响 UI 默认，**不影响表结构**）。
    pub const fn default_hierarchical(self) -> bool {
        matches!(self, WorkKind::Novel)
    }
}

/// 一部作品。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub id: i64,
    pub kind: WorkKind,
    pub title: String,
    /// 目标字数（可空＝不设目标）。
    pub target_words: Option<i64>,
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
