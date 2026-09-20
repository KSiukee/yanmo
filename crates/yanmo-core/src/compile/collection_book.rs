//! 集**两种成品**的排版：合成一个 `docx` / 合成一个纯文本。
//!
//! 与 [`super::collection`]（格式、入口、读稿）分开：那边管"导出什么、按什么顺序读"，
//! 这边只管"排出来长什么样"——两种格式各自成一段，改排版不该动入口。
//!
//! 共同口径：**目录自动生成**（按集内顺序编号）、**每篇各起一页**（txt 里空行隔开）、
//! 篇名不重复（篇首已经摆过，正文里的小节标题照写）。
// i18n-allow-file: 本文件里的中文（"目录""（未命名）"）是**写进导出文件的内容**，
// 跟稿子走、不随界面语言变。

use crate::model::Work;
use crate::store::RenderedFile;

use super::collection::{CollectionFormat, Piece};
use super::docx::{file_stem, ooxml as x, package};
use super::reading::ItemKind;
use super::CompileOptions;

/// 产物里"目录"那一节的标题（**写进文件的内容**，不随界面语言变）。
const CONTENTS_HEADING: &str = "目录";

/// 空名字的占位（写进文件的内容：作者起了名就覆盖掉）。
const UNTITLED: &str = "（未命名）";

/// 空集那一句话（与"空书也有一句话"同一条：不交一个看不懂的空文件）。
const EMPTY_NOTE: &str = "（这一集里还没有篇目）";

/// `docx`：集名（+ 简介）→ 目录 → 分页 → 每篇各起一页。
pub(super) fn docx_of(collection: &Work, pieces: &[Piece], options: &CompileOptions) -> RenderedFile {
    let mut body = x::paragraph(Some("YanmoTitle"), &collection.title);
    if !collection.summary.trim().is_empty() {
        for line in collection.summary.split('\n') {
            if !line.trim().is_empty() {
                body.push_str(&x::paragraph(Some("YanmoPlain"), line.trim()));
            }
        }
    }
    body.push_str(&x::paragraph(Some("YanmoVolume"), CONTENTS_HEADING));
    if pieces.is_empty() {
        // 空集也给一句话（与"空书也有一句话"同一条：不交一个看不懂的空文件）
        body.push_str(&x::paragraph(Some("YanmoPlain"), EMPTY_NOTE));
    }
    for (index, piece) in pieces.iter().enumerate() {
        body.push_str(&x::paragraph(Some("YanmoPlain"), &entry_line(index + 1, &piece.work.title)));
    }
    if !pieces.is_empty() {
        body.push_str(&x::page_break());
    }

    for (index, piece) in pieces.iter().enumerate() {
        if index > 0 {
            body.push_str(&x::page_break());
        }
        body.push_str(&x::paragraph(Some("YanmoVolume"), &piece.work.title));
        if !piece.work.summary.trim().is_empty() {
            for line in piece.work.summary.split('\n') {
                if !line.trim().is_empty() {
                    body.push_str(&x::paragraph(Some("YanmoPlain"), line.trim()));
                }
            }
        }
        body.push_str(&body_of(piece, options));
    }

    if let Some(author) = &options.author {
        body.push_str(&super::docx::author_section(author));
    }

    RenderedFile {
        relative_path: format!(
            "{}/{}.docx",
            CollectionFormat::Docx.folder(),
            file_stem(collection)
        ),
        content: package(&body, &collection.title),
    }
}

/// 纯文本：集名 → 目录 → 每篇顺排（空行隔开，与合并 txt 同一套排版口径）。
pub(super) fn text_of(collection: &Work, pieces: &[Piece], options: &CompileOptions) -> RenderedFile {
    let mut lines: Vec<String> = vec![collection.title.clone(), String::new(), CONTENTS_HEADING.to_string()];
    if pieces.is_empty() {
        lines.push(EMPTY_NOTE.to_string());
    }
    for (index, piece) in pieces.iter().enumerate() {
        lines.push(entry_line(index + 1, &piece.work.title));
    }
    // 不卡字数：整本集子要的是每篇都完整（这里给一个"没有上限"的账）
    let mut no_limit = i64::MAX;
    for piece in pieces {
        lines.push(String::new());
        lines.push(piece.work.title.clone());
        lines.push(String::new());
        for item in &piece.items {
            match item.kind {
                ItemKind::Container => {
                    if !item.title.trim().is_empty() {
                        lines.push(item.title.clone());
                        lines.push(String::new());
                    }
                }
                ItemKind::Chapter => {
                    let repeats = item.title.trim() == piece.work.title.trim();
                    if !item.title.trim().is_empty() && !repeats {
                        lines.push(item.title.clone());
                        lines.push(String::new());
                    }
                    for paragraph in super::merged::take_paragraphs(item, options, &mut no_limit) {
                        lines.push(paragraph.to_string());
                        lines.push(String::new());
                    }
                }
            }
        }
    }
    if let Some(author) = &options.author {
        lines.push(String::new());
        lines.push(super::AUTHOR_HEADING.to_string());
        lines.push(String::new());
        for line in super::author_lines(author) {
            lines.push(line);
        }
    }
    RenderedFile::text(
        format!("{}/{}.txt", CollectionFormat::MergedTxt.folder(), file_stem(collection)),
        crate::store::normalize(&lines.join("\n")),
    )
}

/// 一篇的正文（小节标题照写；**篇名不重复**——它已经在上面的篇首摆过了）。
fn body_of(piece: &Piece, options: &CompileOptions) -> String {
    let mut out = String::new();
    let mut left = options.body_limit.unwrap_or(i64::MAX);
    for item in &piece.items {
        match item.kind {
            ItemKind::Container => {
                if !item.title.trim().is_empty() {
                    out.push_str(&x::paragraph(Some("YanmoChapter"), &item.title));
                }
            }
            ItemKind::Chapter => {
                let repeats = item.title.trim() == piece.work.title.trim();
                if !item.title.trim().is_empty() && !repeats {
                    out.push_str(&x::paragraph(Some("YanmoChapter"), &item.title));
                }
                for paragraph in super::merged::take_paragraphs(item, options, &mut left) {
                    out.push_str(&x::paragraph(None, paragraph));
                }
            }
        }
    }
    out
}

/// 目录里的一行：`1. 篇名`（空名字给个占位，免得目录里出现一个空洞）。
fn entry_line(index: usize, title: &str) -> String {
    let name = if title.trim().is_empty() { UNTITLED } else { title.trim() };
    format!("{index}. {name}")
}


