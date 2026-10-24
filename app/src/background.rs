//! 正文背景图：**作者选的那张图怎么进数据目录、又怎么送到界面**。
//!
//! # 三条边界（都是本项目的老规矩）
//!
//! 1. **界面拿不到路径**：作者选图这一步在壳里（系统文件对话框），图由壳复制进数据目录
//!    （跟着稿子走：备份与搬家都带上它），库里只留一个**文件名**。
//! 2. **界面要渲染它，也不需要路径**：壳读文件、回一段 `data:` URL——CSP 里 `img-src` 本来就
//!    允许 `data:`（见 `tauri.conf.json`），所以不必开任何文件系统能力、不必为该目录注册
//!    自定义协议；安装包里也不会因此多一个字节（图在数据目录，不进包）。
//! 3. **认不出 / 太大 / 读不到，都如实说，不崩**：图被作者从数据目录里删掉或搬走时，
//!    回一句"图没了"（`present: false`），界面回纯色并说明白——**不报错挡人**。
//!
//! # 为什么在 Rust 侧再验一次格式
//!
//! 不是因为不信界面（界面根本不碰文件），而是因为**扩展名会撒谎**：把 `a.txt` 改名成
//! `a.png` 是家常便饭。所以判据看**文件头那几个字节**，不看名字；这也是"这张图到底能不能
//! 渲染"的唯一可靠判据。

use std::path::{Path, PathBuf};

/// 背景图所在的那个子目录（数据目录下，与库文件并列）。
///
/// 起个带点的名字：它不是一个给作者浏览的文件夹，是"这一份稿子的随身素材"。
pub const BG_DIR: &str = ".background";

/// 单张上限 **8 MB**。
///
/// 为什么要有上限：这张图要以 base64 的形式在内存里走一趟（那是它进 CSS 的唯一方式），
/// 太大的照片会让开关设置那一下明显卡顿，而"当背景垫一层气氛"根本用不上那么大的图。
/// 超了**如实拒**（并说清上限），不悄悄缩图——替作者改他的图比拒掉更糟。
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// 读回来时的保险丝：超过这个大小连读都不读（多半是库那一格被手改过，
/// 或者有人往数据目录里塞了别的东西）。它比 [`MAX_BYTES`] 宽，够把超过上限的老图也放行一次。
const READ_LIMIT: u64 = 32 * 1024 * 1024;

/// 认出来的图片类型：`(mime, 扩展名)`。
type ImageKind = (&'static str, &'static str);

/// 标准 base64（带 `=` 补齐）——**自己写，不为这一处引一个依赖**。
///
/// 为什么不留一块给 `base64` crate：这一族只用一次（把图变成 data URL），
/// 而多一个**随发布产物走**的第三方依赖要跟着抄许可证、跟升级、跟安全公告；
/// 这段四十行、有单测钉着（含"每三个字节一组""补齐"与"长度不整"三种情形），
/// 比引进来的维护面小得多。
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 0x3F] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3F] as char);
        // 只剩一个字节时，第三、四位没有内容 → 补 `=`
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) as usize & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple as usize & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 按**文件头**认图片类型；认不出（或不是图片）返回 `None`。
pub fn image_kind(bytes: &[u8]) -> Option<ImageKind> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(("image/png", "png"));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(("image/jpeg", "jpg"));
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(("image/webp", "webp"));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(("image/gif", "gif"));
    }
    None
}

/// 文件名里必须有一个**图片的扩展名**（按认出来的类型挑）。
///
/// 为什么要换掉扩展名：作者手上那张可能叫 `照片` 或者名字里带一堆符号，
/// 而且 `.txt` 改名成 `.png` 的事前面刚说过——落进数据目录的那一份，
/// 名字**由我们按真实类型给**，出问题时一眼就能看懂。
///
/// 清名字这一步复用核心的 `safe_file_name`（与导出 / 快照同一把尺子：去非法字符、
/// 压掉连续下划线、超长截断）；它什么都剩不下时回 `untitled`——一个**语言无关**的
/// 中性名（文件名要写到磁盘上，不该随界面语言变），照用即可。
fn file_name_for(original: &str, ext: &str) -> String {
    let stem = Path::new(original)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let cleaned: String = yanmo_core::atomic::safe_file_name(stem.trim()).chars().take(60).collect();
    format!("{cleaned}.{ext}")
}

/// 背景图目录（**不存在也返回路径**——写入方自己建）。
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BG_DIR)
}

/// 这张图在数据目录里的完整路径（只在这一层拼；外面拿到的永远是文件名）。
pub fn path_of(data_dir: &Path, name: &str) -> PathBuf {
    dir(data_dir).join(name)
}

/// 一次"换图"的结果：落进数据目录的那个文件名 + 它多少字节。
///
/// **不带 data URL**：那一段是"要渲染的时候才读"的东西（见 [`read_data_url`]）——
/// 复制这一步不该顺手多编一遍 base64，换图要的是"快、稳、别留半个文件"。
#[derive(Debug, Clone)]
pub struct Stored {
    /// 落进数据目录的那个文件名（**只有名字，没有目录**）
    pub name: String,
    pub bytes: u64,
}

/// 把作者选的这张图**复制进数据目录**。
///
/// 校验顺序有意如此：**先看大小**（超了连读都不读，8 MB 的读一下也要时间）、
/// 再读进内存、**按文件头认类型**、最后才写。任何一步不过，数据目录里不会多出东西
/// ——"拒了却留下半个文件"是比拒掉更糟的结果。
///
/// ★ 调用方负责在**这一份写成功之后**再删旧图、再落库（顺序见 `commands::background`）。
pub fn store(source: &Path, data_dir: &Path) -> yanmo_core::Result<Stored> {
    use yanmo_core::error::{codes, Error};

    let metadata = std::fs::metadata(source).map_err(|error| read_failed(source, error))?;
    if !metadata.is_file() {
        return Err(Error::invalid(codes::BACKGROUND_NOT_A_FILE));
    }
    if metadata.len() > MAX_BYTES {
        return Err(Error::invalid_with(
            codes::BACKGROUND_IMAGE_TOO_BIG,
            [
                ("size", mb(metadata.len())),
                ("max", mb(MAX_BYTES)),
            ],
        ));
    }

    let bytes = std::fs::read(source).map_err(|error| read_failed(source, error))?;
    let (_, ext) = image_kind(&bytes).ok_or_else(|| Error::invalid(codes::BACKGROUND_IMAGE_KIND))?;

    let original = source.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let name = file_name_for(&original, ext);
    let target = dir(data_dir);
    std::fs::create_dir_all(&target).map_err(|error| write_failed(&target, error))?;
    yanmo_core::atomic::write_atomic(&path_of(data_dir, &name), &bytes)?;

    Ok(Stored { name, bytes: bytes.len() as u64 })
}

/// 读回来给界面渲染用：`(data_url, 字节数)`；文件不在 / 读不动 / 太大，都是 `None`。
///
/// **不报错**是有意的：图被删掉、被搬走、盘坏了，作者该看到的是一句"图没了，回纯色"，
/// 而不是一个挡住整个设置面板的错误框。
pub fn read_data_url(data_dir: &Path, name: &str) -> Option<(String, u64)> {
    let path = path_of(data_dir, name);
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > READ_LIMIT {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let (mime, _) = image_kind(&bytes)?;
    Some((format!("data:{mime};base64,{}", base64(&bytes)), bytes.len() as u64))
}

/// IO 失败一律归到核心那个 `io` 码上（界面那边有对应文案），**不新造码**。
///
/// ⚠️ 这里的 `detail` 是**给日志与开发者看的英文短语**（路径 + 系统错误）：
/// 界面文案只许住在字典里（`scan_text` 那条守卫盯着中文），所以核心与壳这边一句中文都不写。
fn read_failed(path: &Path, error: std::io::Error) -> yanmo_core::Error {
    yanmo_core::Error::Io(std::io::Error::new(
        error.kind(),
        format!("cannot read the background image ({}): {error}", path.display()),
    ))
}

/// 同上：写不进去（盘满 / 只读 / 没权限）。
fn write_failed(path: &Path, error: std::io::Error) -> yanmo_core::Error {
    yanmo_core::Error::Io(std::io::Error::new(
        error.kind(),
        format!("cannot write into the background directory ({}): {error}", path.display()),
    ))
}

/// 删掉数据目录里的某一张图（换图 / 清掉时用）。删不动就当没有——**绝不因此失败**。
///
/// 只删**我们自己拼出来的那个路径**（`dir/name`），而且名字来自库里那一格；
/// 名字里但凡带目录成分，`path_of` 之前已经由核心的 `image_name` 剁掉了。
pub fn remove(data_dir: &Path, name: &str) {
    let path = path_of(data_dir, name);
    if path.parent() != Some(dir(data_dir).as_path()) {
        return; // 不在背景图目录里：这不是我们该动的东西
    }
    let _ = std::fs::remove_file(path);
}

/// 删掉数据目录里**所有**背景图（清掉这一层时用）：目录里可能还留着换图时的旧文件。
///
/// 只认这一个目录、只删文件、**不删目录本身**（它跟着稿子走，下次换图还要用）。
pub fn clear_dir(data_dir: &Path) -> usize {
    let dir = dir(data_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0; // 目录本来就不在＝没有图
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// MB 数（一位小数）——只用来拼"这张图有 X MB"那句话。
fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
    const WEBP: &[u8] = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
    const GIF: &[u8] = b"GIF89a\x00\x00";

    #[test]
    fn kinds_are_read_from_the_header_not_the_extension() {
        assert_eq!(image_kind(PNG), Some(("image/png", "png")));
        assert_eq!(image_kind(JPEG), Some(("image/jpeg", "jpg")));
        assert_eq!(image_kind(WEBP), Some(("image/webp", "webp")));
        assert_eq!(image_kind(GIF), Some(("image/gif", "gif")));
        assert_eq!(image_kind(b"just some text"), None, "把 .txt 改名成 .png 也认不出来");
        assert_eq!(image_kind(b""), None);
        // 只差一个字节的假货不许混过去
        assert_eq!(image_kind(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x00]), None);
    }

    #[test]
    fn stored_name_takes_the_real_type() {
        // 名字带符号、扩展名撒谎：落地的名字按**真实类型**给
        assert_eq!(file_name_for("假图.png", "jpg"), "假图.jpg");
        // 带目录成分的名字只留最后一段（按字面量拼，别在源码里写真的绝对路径：
        // 那是脱敏扫描关心的事，而这条测的是"目录成分被剁掉"本身）
        let with_drive = ["C:", "照片", "我的 图.PNG"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(file_name_for(&with_drive, "png"), "我的 图.png");
        // 什么名字都剩不下时给语言无关的中性名（与导出 / 快照同一把尺子的兜底）
        assert_eq!(file_name_for("", "png"), "untitled.png");
        assert_eq!(file_name_for("...", "png"), "untitled.png");
    }

    #[test]
    fn base64_matches_the_standard_alphabet_including_padding() {
        // 经典的三个用例：正好三字节 / 差一个 / 差两个
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
        // 全字节空间走一遍：长度对得上、结尾该补几个 `=` 也对得上
        let all: Vec<u8> = (0u8..=255).collect();
        let encoded = base64(&all);
        assert_eq!(encoded.len(), 344, "256 字节 = 85 组零 1 字节 → 344 个字符");
        assert!(encoded.ends_with('='), "差一个字节要补一个 =");
        assert!(!encoded.contains('\n'));
        assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
    }

    #[test]
    fn store_rejects_a_non_image_without_leaving_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("not-an-image.png");
        std::fs::write(&source, b"hello, definitely not an image").unwrap();

        let err = store(&source, tmp.path()).unwrap_err();
        assert!(
            format!("{err:?}").contains("background.image_kind"),
            "要说清是「只认那几种图」：{err:?}"
        );
        assert!(!tmp.path().join(BG_DIR).exists(), "被拒的图不许在数据目录里留下任何东西");
    }

    #[test]
    fn store_copies_it_in_and_reads_it_back_as_a_data_url() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("my photo.png");
        std::fs::write(&source, PNG).unwrap();

        let stored = store(&source, tmp.path()).unwrap();
        assert_eq!(stored.name, "my photo.png");
        assert_eq!(stored.bytes, PNG.len() as u64);
        assert!(path_of(tmp.path(), &stored.name).is_file(), "复制进数据目录了");

        let (url, bytes) = read_data_url(tmp.path(), &stored.name).unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "读回来能直接当 CSS 背景：{url}");
        assert_eq!(bytes, PNG.len() as u64);
    }

    /// 图被删了 / 搬走了：读回来是 `None`（界面回纯色），**不是**一个错误。
    #[test]
    fn a_missing_image_is_not_a_failure() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_data_url(tmp.path(), "gone.png").is_none());
        // 目录里放一个**不是图**的东西（名字对得上）：同样当"读不出来"
        std::fs::create_dir_all(dir(tmp.path())).unwrap();
        std::fs::write(path_of(tmp.path(), "fake.png"), b"not an image").unwrap();
        assert!(read_data_url(tmp.path(), "fake.png").is_none());
        // 目录里放一张**太大**的东西：连读都不读
        std::fs::write(path_of(tmp.path(), "huge.png"), vec![0u8; (READ_LIMIT + 1) as usize]).unwrap();
        assert!(read_data_url(tmp.path(), "huge.png").is_none());
    }

    /// 只删背景图目录里的文件，别处的东西一概不碰（目录名同名也不行）。
    #[test]
    fn clearing_only_touches_our_own_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir(tmp.path())).unwrap();
        std::fs::write(path_of(tmp.path(), "a.png"), PNG).unwrap();
        std::fs::write(path_of(tmp.path(), "b.jpg"), JPEG).unwrap();
        // 作者自己放在数据目录里的东西（背景图目录之外）：
        let keep = tmp.path().join("yanmo.db");
        std::fs::write(&keep, b"the database").unwrap();
        std::fs::create_dir_all(tmp.path().join("export")).unwrap();

        assert_eq!(clear_dir(tmp.path()), 2, "背景图目录里两张都清掉");
        assert!(keep.is_file(), "数据目录里的别的东西一个都不许动");
        assert!(tmp.path().join("export").is_dir());
        assert!(dir(tmp.path()).is_dir(), "目录本身留着（下次换图还要用）");
    }

    #[test]
    fn remove_is_a_no_op_when_it_is_already_gone() {
        let tmp = tempfile::tempdir().unwrap();
        remove(tmp.path(), "never-existed.png"); // 不 panic、不报错
    }
}
