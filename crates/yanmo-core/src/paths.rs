//! 路径约定：**数据放在哪、导出到哪**——两个壳共用同一份。
//!
//! 为什么放在核心：项目里有**两个壳**（图形界面与命令行救援入口），它们必须认同一个
//! 数据目录、同一个导出目录。约定各写一份，改一处漏一处，作者就会以为"稿子丢了"。
//!
//! 这里只做**纯计算**（拼路径、读环境变量、问一下目录在不在），不建目录、不写文件。
//! 磁盘上的目录名一律**语言无关**：它们跟着稿子走，不该随界面语言变。

use std::path::{Path, PathBuf};

/// 应用数据目录名（与打包标识一致）。
pub const APP_DIR_NAME: &str = "app.yanmo.desktop";
/// 数据库文件名（位于应用数据目录内）。
pub const DB_FILE: &str = "yanmo.db";
/// 导出目录名：放在作者的"文档"里（他自己找得到的地方），语言无关。
pub const EXPORT_FOLDER: &str = "YanmoExport";
/// 早先版本用过的导出目录名——**磁盘上已经有的数据**，不是界面文案，不能跟着语言走。
pub const LEGACY_EXPORT_FOLDER: &str = "研墨导出";

/// **便携标记**：程序目录里出现这个文件，就表示"数据跟着程序走"。
///
/// 为什么用标记，而不是"程序目录能不能写"来自动判断：exe 被单独放到桌面或下载目录时，
/// 那些目录通常**可写**——自动判定会把稿库落到桌面（最容易误删、最容易被同步盘扫到的地方）。
/// 标记只能由**发布形态**带进来（安装器/绿色包放进去），所以判定是确定的、不靠猜。
pub const PORTABLE_MARKER: &str = "yanmo-portable.txt";

/// 便携模式下数据目录的名字（程序目录里的子目录）。
pub const PORTABLE_DATA_FOLDER: &str = "data";

/// 数据目录定在哪。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDir {
    /// 程序目录旁（便携）：`<程序目录>/data`
    Portable(PathBuf),
    /// 系统数据目录（默认）
    System(PathBuf),
}

/// 程序目录里有没有便携标记（只看，不创建、不修改任何东西）。
pub fn is_portable(exe_dir: &Path) -> bool {
    exe_dir.join(PORTABLE_MARKER).is_file()
}

/// 定下数据目录：带便携标记就走程序目录旁，否则走系统数据目录。
///
/// 返回 `None` 表示"系统数据目录都拿不到"——调用方要给人话提示，而不是自己瞎猜一个位置。
pub fn resolve_data_dir(exe_dir: &Path) -> Option<DataDir> {
    if is_portable(exe_dir) {
        return Some(DataDir::Portable(exe_dir.join(PORTABLE_DATA_FOLDER)));
    }
    default_data_dir().map(DataDir::System)
}

/// 这个目录能不能写——**真的试一下**（建目录 → 写探针 → 删掉）。
///
/// 只看权限位不可靠：Windows 的 ACL、只读介质、以及 UAC 的 VirtualStore 重定向都会骗人。
/// 会顺手把目录建出来——调用方本来就要用它，不算副作用。
pub fn is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".yanmo-write-probe");
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 导出根目录：**老目录还在就继续用它**（把作者已经导出的东西留在原地更好找），
/// 否则用语言无关的新名字。两边各导一份，会让人以为稿子分家了。
pub fn export_root(documents: &Path) -> PathBuf {
    let legacy = documents.join(LEGACY_EXPORT_FOLDER);
    if legacy.is_dir() {
        legacy
    } else {
        documents.join(EXPORT_FOLDER)
    }
}

/// 应用数据目录（按系统约定拼；系统目录拿不到就返回 `None`）。
pub fn default_data_dir() -> Option<PathBuf> {
    platform_data_dir().map(|base| base.join(APP_DIR_NAME))
}

/// 作者的"文档"目录（导出稿子的默认落点；拿不到就返回 `None`）。
pub fn documents_dir() -> Option<PathBuf> {
    platform_documents_dir()
}

#[cfg(windows)]
fn platform_data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
    Some(base).filter(|path| path.is_dir())
}

#[cfg(windows)]
fn platform_documents_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    Some(home.join("Documents")).filter(|path| path.is_dir())
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join("Library/Application Support")).filter(|path| path.is_dir())
}

#[cfg(target_os = "macos")]
fn platform_documents_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join("Documents")).filter(|path| path.is_dir())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        if xdg.is_dir() {
            return Some(xdg);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".local/share")).filter(|path| path.is_dir())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_documents_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join("Documents")).filter(|path| path.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_root_prefers_the_existing_legacy_folder() {
        let dir = tempfile::tempdir().unwrap();
        let documents = dir.path();
        // 什么都没有时用语言无关的新名字
        assert_eq!(export_root(documents), documents.join(EXPORT_FOLDER));
        // 老目录已经在了：继续用它（作者原来导出的东西还在那儿）
        std::fs::create_dir_all(documents.join(LEGACY_EXPORT_FOLDER)).unwrap();
        assert_eq!(export_root(documents), documents.join(LEGACY_EXPORT_FOLDER));
    }

    #[test]
    fn data_dir_is_the_app_directory_under_the_platform_base() {
        let data = default_data_dir().expect("本机应当能取到系统数据目录");
        assert!(data.ends_with(APP_DIR_NAME), "数据目录应以应用标识结尾：{}", data.display());
        assert!(data.parent().is_some_and(|parent| parent.is_dir()), "它的上级应当真实存在");
    }

    #[test]
    fn documents_dir_exists_when_the_platform_has_one() {
        if let Some(documents) = documents_dir() {
            assert!(documents.is_dir(), "拿到的文档目录应当真的存在：{}", documents.display());
        }
    }

    #[test]
    fn without_the_marker_it_stays_on_the_system_directory() {
        let dir = tempfile::tempdir().unwrap();
        let chosen = resolve_data_dir(dir.path()).expect("本机应当能取到系统数据目录");
        assert!(matches!(chosen, DataDir::System(_)), "没有标记就不该走便携：{chosen:?}");
        assert!(!is_portable(dir.path()));
    }

    #[test]
    fn the_marker_switches_to_the_folder_next_to_the_program() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PORTABLE_MARKER), b"portable").unwrap();
        let chosen = resolve_data_dir(dir.path()).expect("有标记就该定下便携目录");
        assert_eq!(chosen, DataDir::Portable(dir.path().join(PORTABLE_DATA_FOLDER)));
        assert!(is_portable(dir.path()));
    }

    #[test]
    fn writability_is_probed_for_real() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("还没有的目录/再深一层");
        assert!(is_writable(&target), "可写位置应当能被建出来并写进去");
        assert!(target.is_dir(), "探测会顺手把目录建好");
        assert!(
            !target.join(".yanmo-write-probe").exists(),
            "探针文件必须清掉，不能留在作者的目录里"
        );
    }
}
