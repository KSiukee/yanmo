//! 合并纯文本：全书按阅读顺序拼成**一个文件**——通读、贴到别处、丢进任何编辑器都能开。
//!
//! 排版口径（与导出的"换行归一、末尾一个换行"一致）：
//! - 书名在最前；
//! - 容器标题（卷）与章标题各占一行；
//! - 章标题与正文之间空一行，段与段之间空一行。

use crate::model::Work;
use crate::store::RenderedFile;

use super::reading::{Item, ItemKind};
use super::CompileOptions;

/// 把读好的清单拼成合并文本。
pub(crate) fn render(work: &Work, items: &[Item], options: &CompileOptions) -> RenderedFile {
    let mut lines: Vec<String> = vec![work.title.clone()];
    let mut left = options.body_limit.unwrap_or(i64::MAX);

    for item in items {
        match item.kind {
            ItemKind::Container => {
                if !item.title.trim().is_empty() {
                    lines.push(item.title.clone());
                }
            }
            ItemKind::Chapter => {
                lines.push(String::new());
                lines.push(item.title.clone());
                for paragraph in take_paragraphs(item, options, &mut left) {
                    lines.push(String::new());
                    lines.push(paragraph.to_string());
                }
            }
        }
    }

    RenderedFile::text(
        format!("{}/{}.txt", super::preset::Preset::MergedTxt.folder(), file_stem(work)),
        crate::store::normalize(&lines.join("\n")),
    )
}

/// 按上限取段落（同一套累加口径，docx 那边也用）。
///
/// 上限按**逐字（含标点）**算：中文投稿按这个口径谈字数。一段都还没装下时，
/// 第一段照装——不然"一章就超限"的书会导出一个空正文。
pub(crate) fn take_paragraphs<'a>(
    item: &'a Item,
    options: &CompileOptions,
    left: &mut i64,
) -> Vec<&'a str> {
    let mut out = Vec::new();
    for paragraph in &item.paragraphs {
        let cost = crate::text::count_chars(paragraph);
        if out.is_empty() && options.body_limit.is_some() && *left <= 0 {
            // 上限用完了：这一段都不装了
            break;
        }
        if options.body_limit.is_some() && cost > *left && !out.is_empty() {
            break;
        }
        *left -= cost;
        out.push(paragraph.as_str());
    }
    out
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
