//! 盘符探测：卷标 / 序列号 / 是不是可移动盘 / 剩余容量。
//!
//! # 为什么单独一块
//!
//! 备份要它（列盘、认"哪块盘"、看放不放得下），换数据位置也要它（选中可移动盘要强警告）。
//! 两处各写一份 `GetDriveTypeW` 迟早会不一致，所以收在这里，**只在这一个文件里跟
//! Windows 的存储 API 打交道**。
//!
//! 纪律：识别"哪块盘"一律用**卷序列号**，不用盘符——换 USB 口盘符会变。

use std::path::Path;

/// 一块盘的现状（给界面看的东西，不是句柄）。
#[cfg(windows)]
pub struct VolumeInfo {
    pub label: String,
    pub volume_id: String,
    pub removable: bool,
}

/// 某个路径所在盘的卷序列号（拿不到就是空串，调用方按"认不出这块盘"处理）。
#[cfg(windows)]
pub fn volume_id_for(path: &Path) -> String {
    let Some(root) = root_of(path) else { return String::new() };
    volume_info(&root).map(|info| info.volume_id).unwrap_or_default()
}

#[cfg(not(windows))]
/// 非 Windows 的占位实现：没有「卷标识」这回事。
pub fn volume_id_for(_path: &Path) -> String {
    String::new()
}

/// 这个路径落在可移动盘 / 网络盘上吗（数据目录选它要强警告：写一半被拔可能坏库）。
#[cfg(windows)]
pub fn is_removable(path: &Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    /// `GetDriveTypeW` 的取值（winbase.h：DRIVE_REMOVABLE = 2、DRIVE_REMOTE = 4）。
    /// 自己写常量而不是从 windows-sys 引：那是 C 宏，不是所有版本都导出。
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_REMOTE: u32 = 4;
    let Some(root) = root_of(path) else { return false };
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    matches!(unsafe { GetDriveTypeW(wide.as_ptr()) }, DRIVE_REMOVABLE | DRIVE_REMOTE)
}

#[cfg(not(windows))]
/// 非 Windows 的占位实现：一律当不可移动盘。
pub fn is_removable(_path: &Path) -> bool {
    false
}

/// 路径所在盘的**挂载点**（盘符加一个反斜杠那种形态）。拿不到（没插盘、路径还不存在）就返回 `None`。
///
/// 用系统的 `GetVolumePathNameW` 而不是自己切盘符：目录挂载点（某个文件夹上挂了另一块盘）
/// 与 UNC 路径都得靠它才认得出。
#[cfg(windows)]
fn root_of(path: &Path) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumePathNameW;
    // 目标目录还不存在时 canonicalize 会失败——退一步用它最浅的祖先
    let probe = path
        .ancestors()
        .find(|candidate| candidate.exists())
        .unwrap_or(path)
        .to_path_buf();
    let wide: Vec<u16> = probe.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let mut root = vec![0u16; 8];
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
    if ok == 0 {
        return None;
    }
    let end = root.iter().position(|unit| *unit == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&root[..end]))
}

/// 卷标 / 序列号 / 是不是可移动盘。
#[cfg(windows)]
pub fn volume_info(root: &str) -> Option<VolumeInfo> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW};
    /// `GetDriveTypeW` 的取值之一（winbase.h：DRIVE_REMOVABLE = 2）。
    const DRIVE_REMOVABLE: u32 = 2;
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut label = vec![0u16; 256];
    let mut serial: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return None; // 光驱没盘、没权限之类：这块跳过就好
    }
    let end = label.iter().position(|unit| *unit == 0).unwrap_or(0);
    let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
    Some(VolumeInfo {
        label: String::from_utf16_lossy(&label[..end]),
        volume_id: format!("{serial:08X}"),
        removable: drive_type == DRIVE_REMOVABLE,
    })
}

/// 剩余 / 总容量（给人看"这个盘还放得下吗"）。
#[cfg(windows)]
pub fn free_space(root: &str) -> (u64, u64) {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let (mut free, mut total) = (0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, std::ptr::null_mut()) };
    if ok == 0 {
        (0, 0)
    } else {
        (free, total)
    }
}

#[cfg(not(windows))]
/// 非 Windows 的占位实现：剩余空间未知（0, 0）。
pub fn free_space(_root: &str) -> (u64, u64) {
    (0, 0)
}
