//! 集导出：把一集里的篇目**按集内顺序**合成一份成品（`docx` / 合并 `txt`）。
//!
//! 与"编译一本书"分开的理由：集不是书——它没有正文，只有**一串引用**。
//! 所以这里读的不是某一棵树，而是集里那几篇各自读一遍再拼起来，并且**自带目录**。
//!
//! 三条口径：
//!
//! - **顺序就是集内顺序**（作者在集视图里排好的那个顺序，不是时间也不是字数）；
//! - **每篇从新的一页开始**（docx 里篇与篇之间分页；txt 里空行隔开）——
//!   编辑拿到集子翻页找篇名才顺手；
//! - **目录自动生成**，不用作者自己数一遍。
//!
//! 幂等与别的预设同一条：不写时间戳、顺序一律照集内顺序，渲染两遍逐字节一样。
// i18n-allow-file: 本文件里的中文（"目录"）是**写进导出文件的内容**，跟稿子走、不随界面语言变。

use crate::error::{codes, Error, Result};
use crate::model::{AuthorBlock, Work, WorkKind};
use crate::store::{RenderedFile, Store};

use super::reading::{self, Item};
use super::CompileOptions;

/// 集导出要哪种成品。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionFormat {
    /// 合成一个 `docx`（目录 + 每篇各起一页）
    Docx,
    /// 合成一个纯文本（目录 + 每篇顺排）
    MergedTxt,
}

/// 产物里"目录"那一节的标题（**写进文件的内容**，不随界面语言变）。


impl CollectionFormat {
    /// 全部格式（顺序就是界面上的顺序）。
    pub const ALL: &'static [CollectionFormat] = &[CollectionFormat::Docx, CollectionFormat::MergedTxt];

    /// 稳定代码（也用作产物子目录名）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Docx => "collection_docx",
            Self::MergedTxt => "collection_txt",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_collection_format`。
    pub fn parse(text: &str) -> Result<Self> {
        Self::ALL.iter().copied().find(|format| format.as_str() == text).ok_or_else(|| {
            Error::invalid_with(codes::UNKNOWN_COLLECTION_FORMAT, [("value", text.to_string())])
        })
    }

    /// 产物子目录（语言无关：它会留在作者的磁盘上）。
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Docx => "collection",
            Self::MergedTxt => "collection_txt",
        }
    }

    /// 产出的文件扩展名（清理旧产物按它认）。
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::MergedTxt => "txt",
        }
    }
}

/// 集里的篇目（读好的那一份）：作品本身 + 它的阅读顺序清单。
///
/// 两种成品的排版都在 [`super::collection_book`]，所以它对同层可见。
pub(super) struct Piece {
    pub(super) work: Work,
    pub(super) items: Vec<Item>,
}

/// 把一个集导出一份成品。
pub(crate) fn render(
    store: &Store,
    collection_id: i64,
    format: CollectionFormat,
    author: Option<AuthorBlock>,
) -> Result<RenderedFile> {
    let collection = store.get_work(collection_id)?;
    if collection.kind != WorkKind::Collection {
        return Err(Error::invalid_with(
            codes::COLLECTION_NOT_A_COLLECTION,
            [("work_id", collection_id.to_string())],
        ));
    }
    let mut pieces = Vec::new();
    for entry in store.collection_view(collection_id)?.entries {
        pieces.push(Piece { items: reading::read(store, entry.id)?, work: store.get_work(entry.id)? });
    }
    // 集导出不卡字数：整本集子要的是**每篇都完整**（那不是投稿版，是成书）
    let options = CompileOptions { body_limit: None, with_outline: false, author };

    Ok(match format {
        CollectionFormat::Docx => super::collection_book::docx_of(&collection, &pieces, &options),
        CollectionFormat::MergedTxt => super::collection_book::text_of(&collection, &pieces, &options),
    })
}

