//! 从备份恢复的命令域：列来源、体检预览、手选一个库文件、给加密包的钥匙、真正换库。
//!
//! # 纪律
//!
//! - **这里一行文件操作都没有**：体检、留底、回滚、换库全在核心（`store::restore`），
//!   壳只管"关库 → 叫核心干活 → 重启"这条编排；唯一的例外是**加密包的临时解包目录**
//!   （它由壳建、由壳清，见 [`backup_restore_unlock`]），因为"把包解开到哪"是壳的事；
//! - **绝不硬恢复**：执行前当场再体检一次——界面看过的那一遍不算数，
//!   这份包可能在预览之后被改坏、被拔盘；
//! - **选文件也不给界面路径能力**：系统自带的打开对话框在壳里调，界面只拿到一个字符串；
//!   连对话框的文案都由界面传进来（界面文案只该有一处出处）；
//! - **加密包不等于坏包**：`.ympkg` 解开之后就是一份标准包目录，走的是**同一条**体检/换库链。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{
    is_sealed_package, scan_packages, PackageBrief, RestoreOutcome, RestorePreview, Store,
    KEEP_FOLDER,
};

/// 可恢复的来源现状：看得见的备份包 + 扫过的位置 + 留底目录（都是给作者看的路径）。
#[derive(Debug, Serialize)]
pub struct RestoreSourcesDto {
    pub packages: Vec<PackageBrief>,
    /// 扫过的备份位置（作者勾过、且这一刻目录真的还在的）
    pub roots: Vec<String>,
    pub data_dir: String,
    /// 换库时原库会留底到哪儿
    pub keep_dir: String,
}

/// 一次"给了钥匙"的结果：体检结论 + **解开的目录**（界面上那句"换库"要把它递回来）。
#[derive(Debug, Serialize)]
pub struct UnsealedSourceDto {
    pub preview: RestorePreview,
    pub source: String,
}

/// 数据目录（库文件所在的那一层）。
fn data_dir_of(data: &AppData) -> PathBuf {
    data.db_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 解包临时目录的名字前缀：`.restore-sealed-<pid>`。**只认这一个名字**。
const SEALED_TEMP_PREFIX: &str = ".restore-sealed-";

/// 解包临时目录的名字（`.restore-sealed-<pid>`）：**造名字与扫名字共用这一个口径**。
fn sealed_temp_name(pid: u32) -> String {
    format!("{SEALED_TEMP_PREFIX}{pid}")
}

/// 加密包解开之后的临时目录（数据目录里；每次解锁前先清、换库后再清）。
pub(crate) fn sealed_temp_dir(data: &AppData) -> PathBuf {
    data_dir_of(data).join(sealed_temp_name(std::process::id()))
}

/// 清掉**本次运行**解开临时用的那份明文（换库成功后清；解锁前也先清一次，见下）。
pub(crate) fn sweep_sealed_temp(data: &AppData) {
    let _ = std::fs::remove_dir_all(sealed_temp_dir(data));
}

/// 数据目录里那些**上一次运行**留下的解包临时目录。
///
/// 名字里带的是**那次运行的 pid**，所以不能拿"本进程 pid 拼出来的那个名字"去找——那必然找不着。
/// 判据窄到只认 `.restore-sealed-<纯数字>` 这一种**目录**名：别人的东西一律不碰。
fn stale_sealed_dirs(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return Vec::new(); // 目录还没建出来（第一次运行）：没什么可扫的
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(SEALED_TEMP_PREFIX))
                .map(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
                .unwrap_or(false)
        })
        .collect()
}

/// 启动时扫一遍：上一次运行崩在"解开之后、换库之前"留下的那份明文，不该一直躺在数据目录里。
///
/// 同一份数据目录同时只允许一个实例（`single::InstanceLock`），所以此刻数据目录里带这个记号的
/// 目录一定是**已经死掉的那次运行**留下的——清掉不会有第二个实例正在用着它。
/// 按名字扫掉残留（纯目录活，与 `AppData` 无关——测试直接敲这一层，免得测试里也写一个删除点）。
fn sweep_stale_dirs(data_dir: &Path) {
    for dir in stale_sealed_dirs(data_dir) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

pub(crate) fn sweep_stale_sealed_temp(data: &AppData) {
    sweep_stale_dirs(&data_dir_of(data));
}

/// 列出能恢复的备份：扫作者勾过的每个备份位置。
///
/// 盘不在就扫不到——**列表本来就该只列"现在真拿得到"的东西**，
/// 列一堆插着盘才能恢复的条目只会让作者点了才发现不行。
///
/// 加密包（`.ympkg`）也在列表里：它的清单在密文里，所以那几栏（书名 / 章数 / 字数 / 机器）
/// 是空的，只报"什么时候写的、多大"——要看清内容得先给钥匙。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_sources(data: State<'_, AppData>) -> Result<RestoreSourcesDto, ApiError> {
    let config = data.with_store(|store: &mut Store| Ok(store.backup_config()?))?;
    let roots: Vec<PathBuf> = config
        .targets
        .iter()
        .map(|target| PathBuf::from(&target.path))
        .filter(|path| path.is_dir())
        .collect();
    let packages = scan_packages(&roots);
    let data_dir = data_dir_of(&data);
    Ok(RestoreSourcesDto {
        packages,
        roots: roots.iter().map(|path| path.to_string_lossy().to_string()).collect(),
        keep_dir: data_dir.join(KEEP_FOLDER).to_string_lossy().to_string(),
        data_dir: data_dir.to_string_lossy().to_string(),
    })
}

/// 恢复前看一眼：这份体不体检通过、换上去会退回几天 / 少多少字。
///
/// ⚠️ 递进来的是**加密包**（`.ympkg`）时明确拒绝：那种包的清单在密文里，
/// 拿它当普通包体检只会说一句"快照打不开"，把人引向错误的方向。
/// 加密包要先走 [`backup_restore_unlock`]（给钥匙 → 解到临时目录）再体检。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_preview(
    data: State<'_, AppData>,
    source: String,
) -> Result<RestorePreview, ApiError> {
    if is_sealed_package(Path::new(&source)) {
        return Err(ApiError::new("backup.restore_sealed_needs_key"));
    }
    data.with_store(|store: &mut Store| Ok(store.preview_restore(Path::new(&source))?))
}

/// 给一个加密包的钥匙：解开到临时目录 → 走**既有**那条体检预览 → 把解开的目录一并报回去。
///
/// 界面拿到的是 `{ preview, source }`：`source` 就是解开的那个目录，后面点「换库」时
/// 把它当普通来源递回 [`backup_restore_apply`] 即可——**换库那条链一个字都不用改**。
///
/// `kind` 两档：`passphrase`（作者记的那句话）/ `recovery`（抄在纸上那一串）。
/// 两把都是同一份数据的钥匙，任一把都行；**给错了只说"打不开"**（不泄露是哪一步错的）。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_unlock(
    data: State<'_, AppData>,
    source: String,
    kind: String,
    secret: String,
) -> Result<UnsealedSourceDto, ApiError> {
    use yanmo_core::error_codes::codes;
    let file = PathBuf::from(&source);
    let secret = match kind.as_str() {
        "recovery" => yanmo_core::seal::Secret::Recovery(secret),
        _ => yanmo_core::seal::Secret::Passphrase(secret),
    };
    let dir = sealed_temp_dir(&data);
    // 上一次留下的先清掉：同一个进程里换一个包再看一遍时，不能让旧内容混进来
    let _ = std::fs::remove_dir_all(&dir);
    yanmo_core::store::open_sealed(&file, &dir, &secret).map_err(|error| {
        let detail = error.to_string();
        match &error {
            // 钥匙不对：只说"打不开"（不泄露是哪一步错的）
            yanmo_core::Error::Invalid { code, .. } if *code == codes::SEAL_OPEN_FAILED => {
                ApiError::new("seal.open_failed").caused_by(detail)
            }
            // 别的失败（磁盘、路径、半截文件）：如实说是"解不开"，同样只说这一句
            _ => ApiError::new("backup.restore_unseal_failed").caused_by(detail),
        }
    })?;
    let preview = data.with_store(|store: &mut Store| Ok(store.preview_restore(&dir)?))?;
    Ok(UnsealedSourceDto { preview, source: dir.to_string_lossy().to_string() })
}

/// 真正换库：**先关库 → 留底 → 换库**，成功后壳自己重启。
///
///
/// 返回成功只意味着"数据目录里已经是备份那一份"——界面该把这句显示给作者，
/// 然后窗口会重启（重启由壳发起，不靠界面记得回来调）。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_apply(
    app: AppHandle,
    data: State<'_, AppData>,
    source: String,
    tz_offset_minutes: i32,
) -> Result<RestoreOutcome, ApiError> {
    let path = PathBuf::from(&source);
    // 当场再体检一次（界面上看过的那遍不算数）
    let preview = data.with_store(|store: &mut Store| Ok(store.preview_restore(&path)?))?;
    if !preview.can_restore {
        // i18n-allow-next-line: 中文分隔符（把若干条诊断串接成一句），不是文案
        let joined = preview.verify.problems.join("；");
        return Err(ApiError::from(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::BACKUP_RESTORE_BLOCKED,
            [("reason", joined)],
        )));
    }
    let outcome = data.restore_apply(&path, tz_offset_minutes)?;
    // 换库成功：加密包解出来的那份明文副本用完就清（作者已经拿到"换好了"）
    sweep_sealed_temp(&data);
    // 给界面一点时间把这句"换好了"显示出来，然后重启。
    // 用后台线程而不是让界面再调一次："界面卡住"不该把软件留在没有库的状态。
    //
    // 单实例锁先**收出来**、在重启前松开：新进程启动时回来要同一把锁，
    // 旧进程不放，新进程就只会看见"已经在运行"而退出——软件就再也回不来了。
    let instance = data.take_instance_lock();
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1200));
        drop(instance);
        handle.restart();
    });
    Ok(outcome)
}

/// 让作者从磁盘上挑一个库文件或加密包（备份位置没配过 / 手上只有一份 `yanmo.db` 时用）。
///
/// 窗口标题与筛选项由界面传进来——**壳不产界面文案**。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_restore_pick(title: String, filter_label: String) -> Option<String> {
    pick_database_file(&title, &filter_label)
}

#[cfg(windows)]
fn pick_database_file(title: &str, filter_label: &str) -> Option<String> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    // 过滤器是"成对的 `\0` 串，最后再来一个 `\0` 收尾"：标签\0*.db;*.ympkg\0\0
    // （加密包也让挑得到：递进来之后界面会先要钥匙，见 backup_restore_unlock）
    let mut filter = wide(filter_label);
    filter.extend(wide("*.db;*.ympkg"));
    filter.push(0);
    let title = wide(title);
    let default_ext = wide("db");
    let mut file = vec![0u16; 4096];

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    // 挂到当前前台窗口上，对话框才会出现在研墨上面而不是背后
    ofn.hwndOwner = unsafe { GetForegroundWindow() };
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = default_ext.as_ptr();
    // OFN_NOCHANGEDIR：不加的话对话框会把**进程的当前目录**改到选中文件那一层，必须拦住
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None; // 作者取消（或系统不给对话框）：静默回去，什么都不做
    }
    let end = file.iter().position(|unit| *unit == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&file[..end]))
}

#[cfg(not(windows))]
fn pick_database_file(_title: &str, _filter_label: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 摆一个现场：目录里混着"该清的"和"碰不得的"。
    fn scene(dir: &Path) {
        for name in [".restore-sealed-123", ".restore-sealed-7", "backups", ".restore-sealed-x"] {
            std::fs::create_dir_all(dir.join(name)).expect("建目录");
        }
        std::fs::create_dir_all(dir.join(".restore-sealed-9").join("inner")).expect("建目录");
        std::fs::write(dir.join(".restore-sealed-456"), b"file not dir").expect("写文件");
        std::fs::write(dir.join("yanmo.db"), b"db").expect("写文件");
    }

    #[test]
    fn only_the_pid_named_directories_count_as_leftovers() {
        let tmp = tempfile::tempdir().expect("临时目录");
        scene(tmp.path());
        let mut found: Vec<String> = stale_sealed_dirs(tmp.path())
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        found.sort();
        // 带纯数字的那个名字才认；`.restore-sealed-x` 不认、**同名的文件**也不认
        assert_eq!(found, vec![".restore-sealed-123", ".restore-sealed-7", ".restore-sealed-9"]);
    }

    #[test]
    fn a_missing_data_directory_is_not_an_error() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let gone = tmp.path().join("还没建出来");
        assert!(stale_sealed_dirs(&gone).is_empty());
    }

    #[test]
    fn sweeping_removes_the_leftovers_and_leaves_everything_else() {
        let tmp = tempfile::tempdir().expect("临时目录");
        scene(tmp.path());
        sweep_stale_dirs(tmp.path());
        assert!(!tmp.path().join(".restore-sealed-123").exists());
        assert!(!tmp.path().join(".restore-sealed-9").exists(), "整棵都该没");
        // 别人的东西一个都不许动
        assert!(tmp.path().join("backups").is_dir());
        assert!(tmp.path().join(".restore-sealed-x").is_dir());
        assert!(tmp.path().join(".restore-sealed-456").is_file());
        assert!(tmp.path().join("yanmo.db").is_file());
    }

    #[test]
    fn the_name_the_unlock_uses_is_the_name_the_sweep_looks_for() {
        let tmp = tempfile::tempdir().expect("临时目录");
        // 这两半边必须是同一个口径：造名字用 sealed_temp_name，扫残留用 stale_sealed_dirs。
        // 口径要是各写各的（比如一边带 pid、一边写死），清理就会永远清不到东西。
        std::fs::create_dir_all(tmp.path().join(sealed_temp_name(4242))).expect("建目录");
        std::fs::create_dir_all(tmp.path().join("研墨的稿库")).expect("建目录");
        let found = stale_sealed_dirs(tmp.path());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].file_name().unwrap().to_string_lossy(),
            ".restore-sealed-4242"
        );
    }
}
