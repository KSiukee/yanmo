//! 让作者挑一个**文件夹**（不是文件）。
//!
//! 与"选一个库文件"（`commands::restore` 里那个）分开：那一个筛的是 `.db` 文件，
//! 这一个要的是目录。两者都只能在壳里调——界面没有文件系统能力。
//!
//! 壳不产界面文案：窗口标题由界面传进来。
//! 作者取消（或系统不给对话框）返回 `None`，**静默回去，什么都不做**。

use std::path::{Path, PathBuf};

/// 打开系统自带的"选择文件夹"对话框。取消返回 `None`。
#[cfg(windows)]
pub fn pick(title: &str) -> Option<PathBuf> {
    use windows_sys::Win32::UI::Shell::{
        ILFree, SHBrowseForFolderW, SHGetPathFromIDListW, BROWSEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // 这几个是 C 宏，windows-sys 不导出——自己写（照盘符探测那边的先例）
    const BIF_RETURNONLYFSDIRS: u32 = 0x0000_0001; // 只列目录
    const BIF_EDITBOX: u32 = 0x0000_0010; // 允许手打路径
    const BIF_NEWDIALOGSTYLE: u32 = 0x0000_0040; // 可拖动、可新建文件夹的老式对话框

    let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // 对话框要一个可写的"显示名"缓冲（它会把选中的名字写进来）
    let mut display = vec![0u16; 260];

    let mut browse: BROWSEINFOW = unsafe { std::mem::zeroed() };
    // 挂到前台窗口上，对话框才会出现在研墨上面而不是背后
    browse.hwndOwner = unsafe { GetForegroundWindow() };
    browse.pszDisplayName = display.as_mut_ptr();
    browse.lpszTitle = title.as_ptr();
    browse.ulFlags = BIF_RETURNONLYFSDIRS | BIF_EDITBOX | BIF_NEWDIALOGSTYLE;

    let pidl = unsafe { SHBrowseForFolderW(&browse) };
    if pidl.is_null() {
        return None; // 作者取消
    }
    let mut buffer = vec![0u16; 4096];
    let ok = unsafe { SHGetPathFromIDListW(pidl, buffer.as_mut_ptr()) };
    // PIDL 是系统分配的内存：不管取路径成没成，都要还回去
    unsafe { ILFree(pidl) };
    if ok == 0 {
        return None;
    }
    let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(0);
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..end]));
    path.is_dir().then_some(path)
}

/// 其余平台还没有发布形态：明确"没有这个能力"，不假装选到了。
#[cfg(not(windows))]
pub fn pick(_title: &str) -> Option<PathBuf> {
    None
}

/// 这个目录里有多少东西（给界面看"这里不是空的"）。
pub fn entry_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}
