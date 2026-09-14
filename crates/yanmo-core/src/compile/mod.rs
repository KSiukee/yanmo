//! 编译管线：**一份原稿 → 一种成品**（投稿版 docx / 分章 txt / 合并 txt）。
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
mod docx;
mod merged;
mod reading;
mod zip;

use crate::error::Result;
use crate::store::{ExportFormat, RenderedFile, Store};

pub use preset::{CompileOptions, Preset};

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
