//! 编译管线：**一份原稿 → 一种成品**（长篇投稿版 docx / 单篇投稿版 docx / 分章 txt / 合并 txt）。
//!
//! 与导出（`store::export`，私有模块不进文档）是同一套路子的两条出口：
//! - 这里只回答"该有哪些文件、每个文件里是什么"（纯函数、可单测、可复现）；
//! - 落盘（原子写、清掉上一次的残留）在壳里。
//!
//! 预设各写**自己的子目录**：换一种编译不会把上一种的产物冲掉。
//!
//! # 幂等
//!
//! 同一份原稿、同一套参数，编译出来的字节必须完全一样：不写时间戳（ZIP 里的时间写死）、
//! 顺序一律照阅读顺序。这样"再编译一次"不会在同步盘或版本工具里冒出一堆噪声。

pub mod preset;
mod collection;
mod collection_book;
mod docx;
mod merged;
mod reading;
mod zip;

// i18n-allow-file: 本文件与下面那几个渲染件里的中文（"作者简介""笔名"等）是
// **写进导出文件的内容**，跟稿子走、不随界面语言变；界面文案在 locales/zh-Hans.json。

use crate::error::Result;
use crate::model::AuthorBlock;
use crate::store::{ExportFormat, RenderedFile, Store};

pub use collection::CollectionFormat;
pub use preset::{CompileOptions, Preset};

/// 文末那一节作者简介的标题（**写进文件的内容**，不随界面语言变）。
pub(crate) const AUTHOR_HEADING: &str = "作者简介";

/// 作者简介那几行**文字**（docx 与纯文本共用同一份口径）。
///
/// 空着的栏**不写、也不补占位**：作者没填电话，稿子里就绝不该出现一行"电话："。
/// 标签是给编辑看的固定写法（与《胶东文学》征稿启事里那串"笔名、真名、电话…"对齐）。
pub(crate) fn author_lines(author: &AuthorBlock) -> Vec<String> {
    let mut out = Vec::new();
    match (author.pen_name.trim(), author.real_name.trim()) {
        ("", "") => {}
        (pen, "") => out.push(format!("笔名：{pen}")),
        ("", real) => out.push(format!("作者：{real}")),
        (pen, real) => out.push(format!("笔名：{pen}　本名：{real}")),
    }
    if !author.phone.trim().is_empty() {
        out.push(format!("电话：{}", author.phone.trim()));
    }
    if !author.email.trim().is_empty() {
        out.push(format!("邮箱：{}", author.email.trim()));
    }
    match (author.address.trim(), author.postcode.trim()) {
        ("", "") => {}
        (address, "") => out.push(format!("通联：{address}")),
        ("", postcode) => out.push(format!("邮编：{postcode}")),
        (address, postcode) => out.push(format!("通联：{address}（邮编 {postcode}）")),
    }
    // 敏感两项（只有作者在设置里勾了"带身份证与银行账号"才会走到这里）
    if !author.id_number.trim().is_empty() {
        out.push(format!("身份证号：{}", author.id_number.trim()));
    }
    if !author.bank.trim().is_empty() {
        out.push(format!("银行账号：{}", author.bank.trim()));
    }
    // 文学简历：几行就几段（作者自己怎么分行就怎么排）
    for line in author.bio.split('\n') {
        if !line.trim().is_empty() {
            out.push(line.trim().to_string());
        }
    }
    out
}

/// 把一个集导出成一份成品（`docx` / 合并 `txt`），返回要写出去的那组文件。
///
/// 与 [`compile`] 分开：集不是书（它没有正文，只有一串引用），
/// 编排方式也不一样——**按集内顺序拼、自带目录、每篇各起一页**。
pub fn compile_collection(
    store: &Store,
    collection_id: i64,
    format: CollectionFormat,
    author: Option<AuthorBlock>,
) -> Result<Vec<RenderedFile>> {
    Ok(vec![collection::render(store, collection_id, format, author)?])
}

/// 按预设编译一本书，返回要写出去的那组文件。
pub fn compile(
    store: &Store,
    work_id: i64,
    preset: Preset,
    options: &CompileOptions,
) -> Result<Vec<RenderedFile>> {
    let work = store.get_work(work_id)?;
    match preset {
        Preset::MergedTxt => {
            let items = reading::read(store, work_id)?;
            Ok(vec![merged::render(&work, &items, options)])
        }
        Preset::SubmissionDocx => {
            let items = reading::read(store, work_id)?;
            Ok(vec![docx::render(&work, &items, options)])
        }
        Preset::ArticleDocx => {
            // 单篇投稿版：书名页 + 正文（不附大纲、不卡字数）
            let items = reading::read(store, work_id)?;
            Ok(vec![docx::render_article(&work, &items, options)])
        }
        Preset::ChaptersTxt => {
            // 分章那条与"导出成文件"共用同一份渲染：一章一个文件、结构用目录表达。
            // 只把路径挪进预设子目录，免得跟别的预设互相覆盖。
            let folder = preset.folder();
            Ok(store
                .render_work(work_id, ExportFormat::Text)?
                .into_iter()
                .map(|file| RenderedFile {
                    relative_path: format!("{folder}/{}", file.relative_path),
                    content: file.content,
                })
                .collect())
        }
    }
}
