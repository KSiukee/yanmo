//! 文件写入的两条纪律：**原子**与**文件名可预测**。
//!
//! # 原子写
//!
//! 同目录临时文件 → `sync_all` 落盘 → `rename` 覆盖。同一文件系统内的 rename 是原子的：
//! 读者要么看到旧文件，要么看到完整的新文件，**永远看不到写了一半的文件**。
//! 顺序不能反——先改名再落盘，崩溃时就可能留下"改了名但内容是空的"的文件。
//!
//! 这里只提供**机制**，不决定写到哪：路径一律由壳（Rust 侧）解析，界面不碰文件系统。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// 临时文件后缀（崩溃后残留的 `.part` 一眼能认出来）。
const TEMP_SUFFIX: &str = ".part";

/// 文件名长度上限（字符数）——防长路径与奇怪的超长标题。
const MAX_NAME_CHARS: usize = 60;

/// 原子写入：临时文件 + 落盘 + rename。
///
/// 目标目录不存在会自动建；写入过程中失败会清掉临时文件，**不留半截文件**。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or_else(|| Error::Invalid(format!("路径没有上级目录：{}", path.display())))?;
    fs::create_dir_all(parent)?;

    let temp = temp_path(path);
    // 任何一步失败都要把临时文件收干净——失败还留一地碎片，下次排查会以为是别的问题
    let result = write_and_sync(&temp, bytes).and_then(|()| fs::rename(&temp, path).map_err(Error::from));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_and_sync(temp: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::File::create(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(TEMP_SUFFIX);
    path.with_file_name(name)
}

/// 把任意标题变成**能安全当文件名**的名字。
///
/// 去掉 Windows 不允许的字符与首尾的点/空格，压掉连续下划线，超长截断；
/// 实在什么都剩不下时给一个中性名字（宁可叫"未命名"，也不要写出非法文件名）。
pub fn safe_file_name(name: &str) -> String {
    let mut cleaned = String::new();
    let mut last_was_underscore = false;
    for ch in name.chars() {
        let mapped = match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        };
        if mapped == '_' {
            if last_was_underscore {
                continue;
            }
            last_was_underscore = true;
        } else {
            last_was_underscore = false;
        }
        cleaned.push(mapped);
    }

    let trimmed = cleaned.trim().trim_matches(['.', '_']).trim();
    let limited: String = trimmed.chars().take(MAX_NAME_CHARS).collect();
    let result = limited.trim().to_string();
    if result.is_empty() {
        "未命名".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn writes_content_and_leaves_no_temp_file() {
        let dir = temp_dir();
        let path = dir.path().join("导出.txt");
        write_atomic(&path, "第一行\n第二行".as_bytes()).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "第一行\n第二行");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(TEMP_SUFFIX))
            .collect();
        assert!(leftovers.is_empty(), "不该留下临时文件：{leftovers:?}");
    }

    #[test]
    fn overwrites_atomically_and_creates_missing_dirs() {
        let dir = temp_dir();
        let path = dir.path().join("逃生").join("恢复.md");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn fails_loudly_when_target_is_a_directory() {
        let dir = temp_dir();
        let blocked = dir.path().join("目录");
        fs::create_dir_all(&blocked).unwrap();
        // 目标是个目录：rename 会失败，必须报错而不是静默丢内容
        assert!(write_atomic(&blocked, b"x").is_err());
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.contains("part"))
            .collect();
        assert!(leftovers.is_empty(), "失败后不该留下临时文件：{leftovers:?}");
    }

    #[test]
    fn file_names_are_predictable_and_legal() {
        assert_eq!(safe_file_name("第一章/雨夜"), "第一章_雨夜");
        assert_eq!(safe_file_name("a<b>c:d\"e|f?g*h"), "a_b_c_d_e_f_g_h");
        assert_eq!(safe_file_name("  拖尾点...  "), "拖尾点");
        assert_eq!(safe_file_name(""), "未命名");
        assert_eq!(safe_file_name("///"), "未命名");
        assert_eq!(safe_file_name(&"很长".repeat(100)).chars().count(), MAX_NAME_CHARS);
    }
}
