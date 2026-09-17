//! 导出 / 编译的**落盘侧**：把渲染好的文件写进目录、清掉上一次的残留、收空目录。
//!
//! # 为什么单独一份
//!
//! 它与"运行期数据句柄"（[`crate::storage::AppData`]）的变化理由不同：那边管数据目录、
//! 单实例、搬迁、会话与退出闸门；这边只管"一次导出怎么落地"。分开之后，
//! 导出那套"内容一样就不重写 / 只清自己的残留 / 绝不越出目录"的口径可以单独读、单独改。
//!
//! 镜像走的是另一条路（`crate::mirror_fs`）：它是持续跟写的，取舍不同（见那边的文件头）。

use std::path::{Path, PathBuf};

use crate::error::ApiError;

/// 一次导出的结果（路径只报给界面看，界面拿到也改不了）。
pub struct ExportOutcome {
    pub dir: PathBuf,
    pub files: usize,
    pub removed: usize,
}

/// 导出/编译落点的文件夹名：`<归一化书名>-<作品 id>`。
///
/// 口径与磁盘镜像**同一处**（`yanmo_core::store::work_folder_name`）：两本同名作品各有各的
/// 目录，后写的那本不会把先写下的当成"这次不再需要的残留"删掉。
pub(crate) fn export_folder_name(work_id: i64, work_title: &str) -> String {
    yanmo_core::store::work_folder_name(work_id, work_title)
}

/// 把一组文件写进 `dir`（按需建目录；**内容一样就不重写**，不白改 mtime）。
///
/// 导出与编译共用这一份：多一处写盘循环，就多一处"忘了改"的机会。
pub(crate) fn write_files(dir: &Path, files: &[yanmo_core::store::RenderedFile]) -> Result<(), ApiError> {
    for file in files {
        let path = dir.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::with(
                    "shell.export_dir_create_failed",
                    [("path", parent.display().to_string())],
                )
                .caused_by(e)
            })?;
        }
        // 比字节：docx 这类产物根本不是 UTF-8 文本，按字符串比会永远"不相等"而反复重写
        let unchanged = std::fs::read(&path).map(|old| old == file.content).unwrap_or(false);
        if unchanged {
            continue;
        }
        yanmo_core::atomic::write_atomic(&path, &file.content).map_err(|e| {
            ApiError::with("shell.export_write_failed", [("path", path.display().to_string())])
                .caused_by(e)
        })?;
    }
    Ok(())
}

/// 导出清单：这个目录里**上一次导出写了哪些文件**（相对路径，一行一个）。
///
/// 放一个点开头的文件，作者在文件管理器里默认看不见它。
const EXPORT_MANIFEST: &str = ".yanmo-export-manifest.txt";

/// 读上一次的导出清单；读不到就当空（**空清单意味着什么都不删**，见 [`prune_export`]）。
pub(crate) fn read_export_manifest(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join(EXPORT_MANIFEST))
        .map(|text| {
            text.lines()
                .map(|line| line.trim().replace('/', std::path::MAIN_SEPARATOR_STR))
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// 写下这一次的导出清单。**失败不报错**：清单只是"下次能清得更准"，
/// 写不下不该让一次成功的导出变成失败。
pub(crate) fn write_export_manifest(dir: &Path, files: &[yanmo_core::store::RenderedFile]) {
    let mut text = files
        .iter()
        .map(|file| file.relative_path.replace('/', std::path::MAIN_SEPARATOR_STR))
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    let _ = std::fs::write(dir.join(EXPORT_MANIFEST), text);
}

/// 清掉**上一次导出留下的孤儿**：只删"上次我们自己写、这次没写"的那些文件。
///
/// 为什么按清单清，而不是"把这个目录里所有不认识的 txt/json 都删掉"：
/// ① 目录名按书名归一化，而**重名作品是允许的**——B 的导出会把 A 刚导出的文件删掉；
/// ② 同一本书的编译产物（`submission/`、`chapters/`、`merged/` 里的 txt）就在同一个目录下，
///    那样扫会把作者刚拿去投稿的那份一并删掉（2026-09-15 代码质量评审：中等 17）。
///
/// 只认自己的清单还有个好处：**升级后第一次导出时清单还是空的，于是什么都不删**——
/// 宁可留几个孤儿，也不误删作者的产物。
///
/// `ours` 由调用方给（导出的产物是 txt/json，编译的产物还可能是 docx）：后缀写死在一处，
/// 换个场景就会误删作者自己放进来的文件。
pub(crate) fn prune_export(
    dir: &Path,
    previous: &[String],
    files: &[yanmo_core::store::RenderedFile],
    ours: &[&str],
) -> Result<usize, ApiError> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let keep: std::collections::HashSet<String> = files
        .iter()
        .map(|file| file.relative_path.replace('/', std::path::MAIN_SEPARATOR_STR))
        .collect();
    let mut removed = 0;
    for relative in previous {
        if keep.contains(relative) {
            continue; // 这次也写了它，留着
        }
        let path = dir.join(relative);
        // 安全阀：清单里的路径必须是**这个目录内的相对路径**（清单文件也可能被人手改）
        if !path.starts_with(dir) || !path.is_file() {
            continue;
        }
        let is_ours = path.extension().is_some_and(|ext| ours.iter().any(|ours| ext == *ours));
        if !is_ours {
            continue;
        }
        std::fs::remove_file(&path).map_err(|e| {
            ApiError::with(
                "shell.export_file_remove_failed",
                [("path", path.display().to_string())],
            )
            .caused_by(e)
        })?;
        removed += 1;
    }
    // 收掉空目录（自下而上，失败就当它还有用，不报错）
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    dirs.push(entry.path());
                    stack.push(entry.path());
                }
            }
        }
    }
    for path in dirs.into_iter().rev() {
        let _ = std::fs::remove_dir(&path);
    }
    Ok(removed)
}
