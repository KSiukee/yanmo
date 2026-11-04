//! 正文字体文件：作者自己挑的 `.ttf` / `.otf` / `.ttc`（**不进安装包**）。
//!
//! 为什么放在壳里：界面不碰文件系统（`boundary` 那条守卫盯着），字体文件要复制、要读字节，
//! 都由壳来做；界面只拿到一个名字和一个 `http://yanmo-font.localhost/...` 的地址。
//!
//! 三条分寸：
//! - **只拷进数据目录**（`<data>/fonts/`），跟着稿子走；不进安装包、不上传、不联网；
//! - **名字由内容算出来**（`font-<hash>.<ext>`）：界面拿到的永远是一个安全文件名，
//!   没有目录成分，也不带作者原来的路径；
//! - 文件头认一下类型：把 `.exe` 改名成 `.ttf` 进不来。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;

/// 单个字体文件的上限：64 MB（够放一个完整的中文字体，也不至于把数据目录塞爆）。
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// 挑完字体之后回给界面的东西：**只有名字与地址，没有路径**。
#[derive(Debug, Serialize)]
pub struct FontPick {
    /// 数据目录里的文件名（安全名，`file:` 后面存的就是它）
    pub name: String,
    /// 给 `@font-face` 用的地址（自定义协议，只服务 `<data>/fonts/` 这一层）
    pub url: String,
}

/// 字体文件放哪儿（与背景图同一个思路：跟着数据目录走）。
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("fonts")
}

fn url_of(name: &str) -> String {
    format!("http://yanmo-font.localhost/local/{name}")
}

/// 名字必须是"我们自己算出来的那种"：没有目录成分、长度有界。
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 80 && !name.contains(['/', '\\', ':'])
}

/// 让作者挑一个字体文件，复制进数据目录并给界面一个可用的地址。
///
/// 作者取消（或系统不给对话框）返回 `None`、静默回去——与"选一张图"同一条路。
#[tauri::command(rename_all = "snake_case")]
pub fn pick_font(
    title: String,
    filter_label: String,
    data: State<'_, AppData>,
) -> Result<Option<FontPick>, ApiError> {
    let Some(source) = pick_font_file(&title, &filter_label) else {
        return Ok(None);
    };
    let name = store(&source, &data.data_dir())?;
    Ok(Some(FontPick { url: url_of(&name), name }))
}

/// 重启之后把地址找回来：文件还在就给地址，被删了就是 `None`（界面回退到字体栈）。
#[tauri::command(rename_all = "snake_case")]
pub fn font_url(name: String, data: State<'_, AppData>) -> Option<String> {
    if !safe_name(&name) {
        return None;
    }
    dir(&data.data_dir()).join(&name).is_file().then(|| url_of(&name))
}

/// 自定义协议：`http://yanmo-font.localhost/local/<名字>`。
///
/// **只认这一层目录里的文件**，名字里带目录成分一律 404；读不到也 404（不报错、不泄露路径）。
pub fn respond(
    data_dir: &Path,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<std::borrow::Cow<'static, [u8]>> {
    use std::borrow::Cow;
    use tauri::http::{Response, StatusCode};

    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Cow::Borrowed(&b""[..]))
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&b""[..])))
    };

    let name = request.uri().path().rsplit('/').next().unwrap_or("");
    if !safe_name(name) {
        return not_found();
    }
    let Ok(bytes) = std::fs::read(dir(data_dir).join(name)) else {
        return not_found();
    };
    let mime = match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "otf" => "font/otf",
        "ttc" => "font/collection",
        _ => "font/ttf",
    };
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store")
        .body(Cow::Owned(bytes))
        .unwrap_or_else(|_| not_found())
}

/// 读文件、验类型、算名字、原子落盘。
fn store(source: &Path, data_dir: &Path) -> Result<String, ApiError> {
    let metadata = std::fs::metadata(source).map_err(io_failed)?;
    if !metadata.is_file() {
        return Err(ApiError::new("value.font_not_a_file"));
    }
    if metadata.len() > MAX_BYTES {
        return Err(ApiError::with("value.font_too_big", []));
    }
    let bytes = std::fs::read(source).map_err(io_failed)?;
    if !looks_like_font(&bytes) {
        return Err(ApiError::new("value.font_kind"));
    }
    let ext = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    let ext = if matches!(ext.as_str(), "ttf" | "otf" | "ttc") { ext } else { "ttf".to_string() };
    let name = format!("font-{:016x}.{ext}", fnv1a(&bytes));
    let target = dir(data_dir);
    std::fs::create_dir_all(&target).map_err(io_failed)?;
    yanmo_core::atomic::write_atomic(&target.join(&name), &bytes)?;
    Ok(name)
}

/// 认字体文件头：TrueType（0x00010000 / `true`）、OpenType（`OTTO`）、集合（`ttcf`）。
///
/// 不解析表结构——那件事留给浏览器；这里只挡住"把别的文件改名进来"。
fn looks_like_font(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
        || bytes.starts_with(b"true")
        || bytes.starts_with(b"OTTO")
        || bytes.starts_with(b"ttcf")
}

/// FNV-1a：名字只要**稳定且够散**（同一份文件重复挑到同一个名字，不重复落盘）。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn io_failed(error: std::io::Error) -> ApiError {
    ApiError::from(yanmo_core::Error::Io(error))
}

/// 让作者挑一个字体文件（与"选一张图"同一手法：**只在壳里调**）。
#[cfg(windows)]
fn pick_font_file(title: &str, filter_label: &str) -> Option<PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    // 过滤器成对：标签\0通配符\0 … 最后再补一个 0 收尾。
    // 标签与标题都由界面传进来（中文文案只许住在 locales/，壳里一个字都不写）
    let mut filter = wide(filter_label);
    filter.extend(wide("*.ttf;*.otf;*.ttc"));
    filter.push(0);
    let title = wide(title);
    let default_ext = wide("ttf");
    let mut file = vec![0u16; 4096];

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = unsafe { GetForegroundWindow() };
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = default_ext.as_ptr();
    // OFN_NOCHANGEDIR：不加的话对话框会把进程的当前目录改到选中文件那一层
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if unsafe { GetOpenFileNameW(&mut ofn) } == 0 {
        return None; // 作者取消
    }
    let end = file.iter().position(|unit| *unit == 0).unwrap_or(0);
    Some(PathBuf::from(String::from_utf16_lossy(&file[..end])))
}

/// 其余平台还没有发布形态：明确"没有这个能力"，不假装选到了。
#[cfg(not(windows))]
fn pick_font_file(_title: &str, _filter_label: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_headers_are_what_get_checked() {
        assert!(looks_like_font(&[0x00, 0x01, 0x00, 0x00, 0, 0]));
        assert!(looks_like_font(b"OTTO...."));
        assert!(looks_like_font(b"ttcf...."));
        assert!(!looks_like_font(b"MZ\x90\x00"), "把 exe 改名成 ttf 进不来");
        assert!(!looks_like_font(b""));
    }

    #[test]
    fn names_have_no_directory_parts() {
        assert!(safe_name("font-0123456789abcdef.ttf"));
        assert!(!safe_name("../secret.ttf"));
        assert!(!safe_name("C:secret.ttf"));
        assert!(!safe_name(""));
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn protocol_serves_only_the_fonts_directory() {
        let dir = tempfile::tempdir().unwrap();
        let fonts = dir.path().join("fonts");
        std::fs::create_dir_all(&fonts).unwrap();
        std::fs::write(fonts.join("font-abc.ttf"), b"OTTO....").unwrap();

        let ok = tauri::http::Request::builder()
            .uri("http://yanmo-font.localhost/local/font-abc.ttf")
            .body(Vec::new())
            .unwrap();
        assert_eq!(respond(dir.path(), ok).status(), 200);

        // 目录成分一律 404（不许拿它去读数据目录里的别的东西）
        let escape = tauri::http::Request::builder()
            .uri("http://yanmo-font.localhost/local/../yanmo.db")
            .body(Vec::new())
            .unwrap();
        assert_eq!(respond(dir.path(), escape).status(), 404);
    }
}
