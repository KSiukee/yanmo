//! 投稿版 docx：**正文（可限字数）+ 大纲**，按阅读顺序排，最后打成一个 ZIP 包。
//!
//! 四件事说清楚：
//! - **正文上限**按段落累加（逐字含标点口径），不把一个自然段切成两半；
//! - **故事总纲**（作者写了才有）在大纲前面单独一节——整本书讲什么，编辑先看这一段；
//! - **大纲**照阅读顺序列卷/章标题与每章那句话（没写就不补占位）；
//! - **能确定怎么排的都排完再打包**：同一次编译渲染两遍必须逐字节一样（导出幂等铁律）。

use crate::model::Work;
use crate::store::RenderedFile;

use super::reading::{Item, ItemKind};
use super::zip::{write_stored, Entry};
use super::{preset::Preset, CompileOptions};
use ooxml as x;

mod ooxml;

/// 排一份投稿版 docx。
pub(crate) fn render(work: &Work, items: &[Item], options: &CompileOptions) -> RenderedFile {
    let mut body = x::paragraph(Some("YanmoTitle"), &work.title);
    if !work.summary.trim().is_empty() {
        for line in work.summary.split('\n') {
            if !line.trim().is_empty() {
                body.push_str(&x::paragraph(Some("YanmoPlain"), line.trim()));
            }
        }
    }

    // 正文：按阅读顺序，遇到上限就停（标题仍然照写——作者要知道断在哪儿）
    let mut left = options.body_limit.unwrap_or(i64::MAX);
    for item in items {
        match item.kind {
            ItemKind::Container => {
                if !item.title.trim().is_empty() {
                    body.push_str(&x::paragraph(Some("YanmoVolume"), &item.title));
                }
            }
            ItemKind::Chapter => {
                if !item.title.trim().is_empty() {
                    body.push_str(&x::paragraph(Some("YanmoChapter"), &item.title));
                }
                for paragraph in super::merged::take_paragraphs(item, options, &mut left) {
                    body.push_str(&x::paragraph(None, paragraph));
                }
            }
        }
    }

    if options.with_outline {
        // 故事总纲：整本书那几段（作者没写就整节不出现——绝不补占位句子）
        if !work.storyline.trim().is_empty() {
            body.push_str(&x::paragraph(Some("YanmoTitle"), x::STORYLINE_HEADING));
            for line in work.storyline.split('\n') {
                if !line.trim().is_empty() {
                    body.push_str(&x::paragraph(Some("YanmoPlain"), line.trim()));
                }
            }
        }
        body.push_str(&x::paragraph(Some("YanmoTitle"), x::OUTLINE_HEADING));
        for item in items {
            if let Some(line) = item.outline_line() {
                body.push_str(&x::paragraph(Some("YanmoPlain"), &line));
            }
        }
    }

    let document = x::document(&body);
    let core = x::core_properties(&work.title);
    let files = [
        Entry { name: "[Content_Types].xml", data: x::CONTENT_TYPES.as_bytes() },
        Entry { name: "_rels/.rels", data: x::ROOT_RELS.as_bytes() },
        Entry { name: "docProps/core.xml", data: core.as_bytes() },
        Entry { name: "docProps/app.xml", data: x::APP_PROPERTIES.as_bytes() },
        Entry { name: "word/_rels/document.xml.rels", data: x::DOC_RELS.as_bytes() },
        Entry { name: "word/styles.xml", data: x::STYLES.as_bytes() },
        Entry { name: "word/settings.xml", data: x::SETTINGS.as_bytes() },
        Entry { name: "word/document.xml", data: document.as_bytes() },
    ];
    let package = write_stored(&files);

    RenderedFile {
        relative_path: format!("{}/{}.docx", Preset::SubmissionDocx.folder(), file_stem(work)),
        content: package,
    }
}

/// 文件名用的书名（去掉路径分隔符这类会捣乱的字）。
fn file_stem(work: &Work) -> String {
    let cleaned = crate::atomic::safe_file_name(&work.title);
    if cleaned.trim().is_empty() {
        "work".to_string()
    } else {
        cleaned
    }
}
