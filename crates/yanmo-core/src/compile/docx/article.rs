//! 单篇投稿版 docx：**书名页 + 正文**（一份短稿一个文件，不附大纲）。
//!
//! 与长篇投稿版（[`super::render`]）差在三处，都来自"短篇怎么投"：
//!
//! - **书名页**：标题（居中、大号）与简介紧挨着摆在第一页，然后**分页**进正文——
//!   编辑翻开第一页就知道这是哪一篇；
//! - **不附大纲**：短篇没有卷章可列（正文里的小节标题照写）；
//! - **不卡字数**：短篇本来就短，切一半反而是把稿子弄残。
//!
//! 排版仍走同一套样式（宋体五号、行距 1.5、段首缩进两字符、纸面不着色），
//! 所以同一篇稿子走哪个预设，编辑看到的正文长得一样。

use crate::model::Work;
use crate::store::RenderedFile;

use super::super::reading::{Item, ItemKind};
use super::super::{merged, preset::Preset, CompileOptions};
use super::ooxml as x;
use super::{file_stem, package};

/// 排一份单篇投稿版 docx。
pub(crate) fn render(work: &Work, items: &[Item], options: &CompileOptions) -> RenderedFile {
    let mut body = x::paragraph(Some("YanmoTitle"), &work.title);
    // 简介（作者写了才有）：书名页上紧跟着标题——它是"给别人看的"，摆在这儿最合位置
    if !work.summary.trim().is_empty() {
        for line in work.summary.split('\n') {
            if !line.trim().is_empty() {
                body.push_str(&x::paragraph(Some("YanmoPlain"), line.trim()));
            }
        }
    }
    body.push_str(&x::page_break());

    let mut left = options.body_limit.unwrap_or(i64::MAX);
    let root = work.title.trim();
    for item in items {
        match item.kind {
            ItemKind::Container => {
                if !item.title.trim().is_empty() {
                    body.push_str(&x::paragraph(Some("YanmoVolume"), &item.title));
                }
            }
            ItemKind::Chapter => {
                // 单篇的根节点**就是这一篇**：标题已经在书名页上摆过了，正文里不再重复。
                // （带小节的长文里，小节标题照写。）
                let repeats_title = item.title.trim() == root;
                if !item.title.trim().is_empty() && !repeats_title {
                    body.push_str(&x::paragraph(Some("YanmoChapter"), &item.title));
                }
                for paragraph in merged::take_paragraphs(item, options, &mut left) {
                    body.push_str(&x::paragraph(None, paragraph));
                }
            }
        }
    }

    if let Some(author) = &options.author {
        body.push_str(&super::author_section(author));
    }

    RenderedFile {
        relative_path: format!("{}/{}.docx", Preset::ArticleDocx.folder(), file_stem(work)),
        content: package(&body, &work.title),
    }
}
