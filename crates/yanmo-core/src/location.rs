//! 「稿子放在哪」：**位置记录**（指针文件）+ 推荐值 + 定夺顺序。
//!
//! # 为什么要有这个模块
//!
//! 稿子放在哪，是作者最在意、也最容易出事的一件事。放在哪不能靠"程序目录能不能写"
//! 去猜（exe 被丢到桌面时那种目录也可写，猜出来的结果是稿库落在最容易误删的地方），
//! 也不能让两个壳各写一份判定（改一处漏一处，作者会以为稿子丢了）。
//!
//! 所以规则收在这里，只有一套：
//!
//! 1. **位置记录最优先**：作者自己选过的地方，写在小文件里，**永远第一个人说话**；
//!    记录指向的目录哪怕现在不在（U 盘没插），也**绝不静默换地方**——只报错。
//! 2. **老位置认领**：没有记录、但老位置已经躺着库（从老版本升上来的作者）→ 就地认领，
//!    顺手把记录补上。老作者不该看见"首次使用"。
//! 3. **首启推荐**：什么都没有才是第一次用——给一个推荐值和理由，让作者确认或另选。
//!
//! # 副作用边界
//!
//! 本模块**只读写那一个小记录文件**（`yanmo-location.txt`），不建数据目录、不动库。
//! 目录能不能写由 [`crate::paths::is_writable`] 去真探，搬迁在 [`crate::store::relocate`]。

use std::path::{Path, PathBuf};

use crate::atomic;
use crate::error::{Error, Result};
use crate::paths;

/// 数据目录是谁定的——界面按它决定要不要弹首启引导。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirSource {
    /// 位置记录说的（作者自己选过）——**最高优先**。
    Recorded,
    /// 没有记录，但程序旁躺着库（绿色包/便携形态的老用户）。
    LegacyPortable,
    /// 没有记录，但系统数据目录里躺着库（安装版的老用户）。
    LegacySystem,
    /// 第一次用：什么记录都没有。
    FirstRun,
}

impl DirSource {
    /// 是不是"第一次用"（界面据此弹首启引导）。
    pub const fn is_first_run(self) -> bool {
        matches!(self, DirSource::FirstRun)
    }
}

/// 定下来的数据目录 + 它是怎么定下来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLocation {
    pub dir: PathBuf,
    pub source: DirSource,
}

/// 首启推荐一个位置，以及**为什么推荐它**（给码，界面对字典翻人话）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// 程序旁（便携形态：数据跟着程序走）
    Portable,
    /// 作者的"文档"里（他自己找得到的地方）
    Documents,
    /// "文档"被同步盘接管了，改推荐主目录——**同步盘上跑库会撞坏稿子**
    DocumentsSynced,
    /// 连"文档"都问不到，放主目录
    Profile,
}

impl Reason {
    /// 界面字典用的码（核心不生产界面文案）。
    pub const fn code(self) -> &'static str {
        match self {
            Reason::Portable => "portable",
            Reason::Documents => "documents",
            Reason::DocumentsSynced => "documents_synced",
            Reason::Profile => "profile",
        }
    }
}

/// 首启推荐的落点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub dir: PathBuf,
    pub reason: Reason,
}

/// 定夺数据目录时要问的全部事实（环境变量 + 壳能问到的系统目录）。
///
/// `documents` / `desktop` 由**壳**填（Tauri 的路径 API 走 Windows 已知文件夹，
/// 能拿到被 OneDrive 重定向后的真值）；命令行救援入口没有壳，就用 [`Sources::from_env`]。
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub exe_dir: PathBuf,
    /// 老版本用的系统数据目录（`%APPDATA%\app.yanmo.desktop`）
    pub system_dir: Option<PathBuf>,
    pub documents: Option<PathBuf>,
    pub desktop: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    /// 同步盘根目录（OneDrive 之类）：命中就**不该**把库放进去
    pub synced_roots: Vec<PathBuf>,
}

impl Sources {
    /// 只靠环境变量问出事实（命令行救援入口用这条路）。
    pub fn from_env(exe_dir: PathBuf) -> Self {
        Self {
            exe_dir,
            system_dir: paths::default_data_dir(),
            documents: paths::documents_dir(),
            desktop: None,
            profile: paths::home_dir(),
            synced_roots: synced_roots_from_env(),
        }
    }

    /// 用壳问到的"文档"（更准）覆盖环境变量拼出来的那个。
    pub fn with_documents(mut self, documents: Option<PathBuf>) -> Self {
        if documents.is_some() {
            self.documents = documents;
        }
        self
    }

    /// 用壳问到的"桌面"覆盖（"选的位置在不在桌面上"靠它判）。
    pub fn with_desktop(mut self, desktop: Option<PathBuf>) -> Self {
        if desktop.is_some() {
            self.desktop = desktop;
        }
        self
    }
}

/// 系统数据目录里那个"位置记录"文件——**它本身不在数据目录里**，否则就自相矛盾了。
///
/// 便携形态放在程序旁：U 盘换台机器插上，记录跟着程序走，还能指回作者自己选的地方。
///
/// 记录的位置取自 [`Sources::system_dir`]（而不是直接读环境变量）：定夺与记录必须
/// **看同一个系统目录**，否则"写在一个地方、读到另一个地方"就成了另一类丢稿。
pub fn pointer_path(sources: &Sources) -> Option<PathBuf> {
    if paths::is_portable(&sources.exe_dir) {
        return Some(sources.exe_dir.join(paths::LOCATION_FILE));
    }
    sources.system_dir.as_ref().map(|dir| dir.join(paths::LOCATION_FILE))
}

/// 读位置记录：文件不在 / 读不出来 / 不是绝对路径 / 空文件 → `None`。
///
/// **坏记录一律当没记录**：宁可走"老位置认领/首启"，也不能拿一行读不懂的字符
/// 把稿子建到莫名其妙的地方去。
pub fn read_record(pointer: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(pointer).ok()?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))?;
    let dir = PathBuf::from(line.trim_matches('"'));
    dir.is_absolute().then_some(dir)
}

/// 写位置记录（原子写：写一半断电也不会留下半行路径）。
pub fn write_record(pointer: &Path, dir: &Path) -> Result<()> {
    // 记录里只放路径一行；上面一行注释是给偶尔打开它的作者看的
    // i18n-allow-next-line: 写进磁盘文件的注释（给作者看的文件内容），不是界面文案
    let body = format!(
        "# 研墨的稿子放在这里（改这一行等于换位置，请保证目录存在）\n{}\n",
        dir.display()
    );
    atomic::write_atomic(pointer, body.as_bytes()).map_err(|error| {
        Error::invalid_with(
            crate::error_codes::codes::LOCATION_RECORD_WRITE_FAILED,
            [("path", pointer.display().to_string()), ("detail", error.to_string())],
        )
    })
}

/// 首启推荐：便携形态推荐程序旁，其余推荐"文档/研墨"（被同步盘接管时改主目录）。
pub fn suggest(sources: &Sources) -> Option<Suggestion> {
    if paths::is_portable(&sources.exe_dir) {
        return Some(Suggestion {
            dir: sources.exe_dir.join(paths::PORTABLE_DATA_FOLDER),
            reason: Reason::Portable,
        });
    }
    let name = paths::DATA_FOLDER_NAME;
    if let Some(documents) = sources.documents.as_ref() {
        if !is_synced(documents, &sources.synced_roots) {
            return Some(Suggestion { dir: documents.join(name), reason: Reason::Documents });
        }
        // "文档"被 OneDrive 之类接管了：SQLite 在同步盘上被反复上传/回滚会撞坏库，
        // 改推荐主目录（同样好找、且不在同步范围里）
        if let Some(profile) = sources.profile.as_ref() {
            return Some(Suggestion {
                dir: profile.join(name),
                reason: Reason::DocumentsSynced,
            });
        }
        return Some(Suggestion { dir: documents.join(name), reason: Reason::Documents });
    }
    sources
        .profile
        .as_ref()
        .map(|profile| Suggestion { dir: profile.join(name), reason: Reason::Profile })
}

/// 定夺数据目录（顺序见模块头注释）。返回 `None` 表示连推荐位置都拼不出来。
pub fn resolve(sources: &Sources) -> Option<DataLocation> {
    let pointer = pointer_path(sources)?;
    if let Some(dir) = read_record(&pointer) {
        // 记录就是记录：**目录现在不在也不换地方**（U 盘没插、盘符变了都是作者能处理的事），
        // 静默换地方才是真灾难——作者会以为稿子没了。
        return Some(DataLocation { dir, source: DirSource::Recorded });
    }
    if paths::is_portable(&sources.exe_dir) {
        let dir = sources.exe_dir.join(paths::PORTABLE_DATA_FOLDER);
        if has_database(&dir) {
            return Some(DataLocation { dir, source: DirSource::LegacyPortable });
        }
    }
    if let Some(system) = sources.system_dir.as_ref() {
        if has_database(system) {
            return Some(DataLocation { dir: system.clone(), source: DirSource::LegacySystem });
        }
    }
    suggest(sources).map(|suggestion| DataLocation {
        dir: suggestion.dir,
        source: DirSource::FirstRun,
    })
}

/// 这个目录里有库吗（只看有没有那个文件，不打开）。
pub fn has_database(dir: &Path) -> bool {
    dir.join(paths::DB_FILE).is_file()
}

/// 选位置时要提醒作者的风险（给码，界面翻人话；`Removable` 由壳探测后补上）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// 在同步盘里（OneDrive / 坚果云 / Dropbox…）
    Synced,
    /// 在桌面上
    Desktop,
    /// 直接放在盘根（会污染根目录，也不便备份）
    DriveRoot,
    /// 在当前数据目录里面（搬迁会被拒——不能往自己里面搬）
    InsideData,
    /// 那里已经有一份稿子库（搬迁会被拒，免得覆盖）
    Occupied,
    /// 可移动盘 / 网络盘（写一半被拔可能坏库）
    Removable,
}

impl Risk {
    pub const fn code(self) -> &'static str {
        match self {
            Risk::Synced => "synced",
            Risk::Desktop => "desktop",
            Risk::DriveRoot => "drive_root",
            Risk::InsideData => "inside_data",
            Risk::Occupied => "occupied",
            Risk::Removable => "removable",
        }
    }
}

/// 纯路径层面的风险清单（不碰磁盘；`Removable` 要壳问系统，见 [`Risk`]）。
pub fn risks(target: &Path, sources: &Sources, current: &Path) -> Vec<Risk> {
    let mut out = Vec::new();
    if is_synced(target, &sources.synced_roots) {
        out.push(Risk::Synced);
    }
    if let Some(desktop) = sources.desktop.as_ref() {
        if under(target, desktop) {
            out.push(Risk::Desktop);
        }
    }
    if target.parent().is_none() {
        out.push(Risk::DriveRoot);
    }
    if under(target, current) {
        out.push(Risk::InsideData);
    }
    if has_database(target) {
        out.push(Risk::Occupied);
    }
    out
}

/// 同步盘根目录（目前靠环境变量：OneDrive 的家用/商用/消费者三种变量名）。
pub fn synced_roots_from_env() -> Vec<PathBuf> {
    ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"]
        .iter()
        .filter_map(|key| std::env::var_os(key))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .collect()
}

/// `path` 是否在 `root` 里面（或就是它）。Windows 上大小写不敏感。
fn is_synced(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| under(path, root))
}

/// 前缀比较：按**路径分量**比——避免名字只差最后几个字的两个目录被当成同一个
/// （`.../稿子2` 不该被算作在 `.../稿子` 里面）。
pub(crate) fn under(path: &Path, root: &Path) -> bool {
    let parts = |value: &Path| -> Vec<String> {
        value
            .components()
            .map(|part| {
                let text = part.as_os_str().to_string_lossy().to_string();
                if cfg!(windows) {
                    text.to_lowercase()
                } else {
                    text
                }
            })
            .collect()
    };
    let (path, root) = (parts(path), parts(root));
    !root.is_empty() && path.len() >= root.len() && path[..root.len()] == root[..]
}
