//! 用系统文件管理器打开一个文件夹。
//!
//! # 为什么放在壳里
//!
//! 界面**没有文件系统能力**（权限集里连 `core:path` 都不给）。而「打开稿子所在的文件夹」
//! 是作者高频想要的动作——所以由壳代劳。两条边界：
//!
//! - **命令不接受路径参数**：只打开壳自己持有的那个数据目录，界面无从指定别处；
//! - **不做任何文件操作**：只是"让系统去显示这个文件夹"，不改动里面的东西。

use std::path::Path;

/// 让系统用默认方式打开这个文件夹（Windows 上是资源管理器）。
///
/// 失败时把技术原因回给调用方（进 `detail`），由界面渲染成人话。
#[cfg(windows)]
pub fn open(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    /// `SW_SHOWNORMAL`（winuser.h：1）。自己写常量而不是从 windows-sys 引：
    /// 那是 C 宏，不是所有版本都导出（照盘符探测那边的先例）。
    const SW_SHOWNORMAL: i32 = 1;

    let wide =
        |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let operation = wide("open");
    let file = wide(&path.to_string_lossy());
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // 文档规定的判据：返回值 <= 32 就是失败（不是句柄）
    let code = result as isize;
    if code <= 32 {
        Err(format!("ShellExecuteW returned {code}"))
    } else {
        Ok(())
    }
}

/// 其余平台还没有发布形态：明确报"不支持"，**不假装打开成功**。
#[cfg(not(windows))]
pub fn open(_path: &Path) -> Result<(), String> {
    Err("opening folders is only supported on the released platform".to_string())
}
