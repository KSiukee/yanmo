//! 备份命令域：偏好读写、立即备份、账本，以及**列出盘符**。
//!
//! 业务全在核心（`store::backup`：一致性快照、读回体检、保留滚动、账本）；
//! 这里只做两件事：
//!
//! 1. 把界面要的**盘**列出来——底层探测在 `crate::volume`（卷标/序列号/是否可移动），
//!    本模块只管拼成界面要的形状；
//! 2. 把参数递进核心、把结果递回界面。
//!
//! 安全边界提醒：备份**失败绝不阻断写作与关窗**——每个目标各自成败，逐目标回报。

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{
    gaps_for, has_other_volume, read_ledger, BackupConfig, BackupReport, BackupRequest, BackupTarget,
    Store,
};
use yanmo_core::time::{local_date, now_millis};

/// 一块盘：界面据此给作者列勾选项（**默认一个都不预勾**）。
#[derive(Debug, Serialize)]
pub struct VolumeDto {
    /// 盘根（形如「一个盘符加一个反斜杠」）——只用来显示与拼默认目录，**识别一律用卷序列号**
    pub root: String,
    pub label: String,
    /// 卷序列号（十六进制）；取不到就是空串
    pub volume_id: String,
    /// 可移动盘（U 盘 / 移动硬盘）——写完之后提示"可以安全拔出"
    pub removable: bool,
    pub free_bytes: u64,
    pub total_bytes: u64,
    /// 数据目录在不在这块盘上（同盘也不排除，只是标注一下防护范围）
    pub holds_data: bool,
}

/// 一个目标的现状（设置页要"最后一次成功是哪天 / 缺了哪几天 / 上回为什么没成"）。
#[derive(Debug, Serialize)]
pub struct TargetStatusDto {
    pub path: String,
    pub volume_label: String,
    /// 最近一次成功的日期（本地 `2026-09-13`）；从没成功过是 null
    pub last_success: Option<String>,
    /// 最近一次没成的原话（成功之后清掉）
    pub last_problem: Option<String>,
    /// 这块盘此刻在不在（**按卷序列号认**；不在才是真的"没插/被拔了"）
    pub volume_present: bool,
    /// 目标目录建了没有——**没建是常态**（第一次备份会自动创建），不该当成故障
    pub dir_exists: bool,
    /// 最近 7 天里缺了哪几天
    pub gaps: Vec<String>,
}

/// 设置页/提示条要的一整份现状。
#[derive(Debug, Serialize)]
pub struct BackupStatusDto {
    pub config: BackupConfig,
    pub volumes: Vec<VolumeDto>,
    /// 数据目录所在盘的卷序列号
    pub data_volume_id: String,
    /// 目标里有没有"异盘"的（没有就该提示作者插盘 / 加目标）
    pub has_other_volume: bool,
    /// 该不该弹那条非阻塞小条（没有异盘目标且作者没拒绝过）
    pub should_nudge: bool,
    pub targets: Vec<TargetStatusDto>,
    pub today: String,
}

/// 这个目标的盘此刻在不在。
///
/// 有卷序列号就按它认（盘符变来变去也不慌）；老配置没记序列号，就退回"盘根在不在"——
/// **注意不是"目标目录在不在"**：还没备份过的目标目录本来就不存在，那不是故障。
fn target_volume_present(target: &BackupTarget, present_ids: &[&str]) -> bool {
    if !target.volume_id.is_empty() {
        return present_ids.contains(&target.volume_id.as_str());
    }
    volume_root(&target.path).is_dir()
}

/// 目标路径所在盘的根（形如"一个盘符加一个反斜杠"）；取不出来就退回原路径。
fn volume_root(path: &str) -> PathBuf {
    let mut root = PathBuf::from(path);
    while let Some(parent) = root.parent() {
        if parent.as_os_str().is_empty() {
            break;
        }
        root = parent.to_path_buf();
    }
    root
}

/// 当前机器名（多机共用一个备份盘时，一眼看出这份备份是谁写的）。
fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "本机".to_string())
}

/// 读现状：配置 + 可见的盘 + 每个目标的账本摘要 + 缺口。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_status(
    data: State<'_, AppData>,
    tz_offset_minutes: i32,
) -> Result<BackupStatusDto, ApiError> {
    crate::acceptance::note_command("backup_status");
    let config = data.with_store(|store: &mut Store| Ok(store.backup_config()?))?;
    let data_dir = data
        .db_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    // 账本是**库之外的 JSON**：库打不开时也读得到（这正是它存在的理由）
    let ledger = read_ledger(&data_dir);
    let today = local_date(now_millis(), tz_offset_minutes);

    let volumes = list_volumes(&data_dir);
    let data_volume_id = crate::volume::volume_id_for(&data_dir);
    // 盘在不在按**卷序列号**判（盘符会变）；拿不到序列号的老配置退回"盘根在不在"
    let present_ids: Vec<&str> = volumes.iter().map(|v| v.volume_id.as_str()).collect();
    let targets = config
        .targets
        .iter()
        .map(|target| {
            let last_success = ledger.last_success(&target.path).map(|e| e.date.clone());
            let last_problem = ledger.last_failure(&target.path).and_then(|e| {
                // 成功过之后就不必再提旧问题
                if last_success.is_none() {
                    Some(e.reason.clone())
                } else {
                    None
                }
            });
            TargetStatusDto {
                path: target.path.clone(),
                volume_label: target.volume_label.clone(),
                last_success,
                last_problem,
                volume_present: target_volume_present(target, &present_ids),
                dir_exists: Path::new(&target.path).is_dir(),
                gaps: gaps_for(&ledger, &target.path, &today, 7),
            }
        })
        .collect();

    Ok(BackupStatusDto {
        has_other_volume: has_other_volume(&config, &data_volume_id),
        should_nudge: !config.tip_dismissed && !has_other_volume(&config, &data_volume_id),
        config,
        volumes,
        data_volume_id,
        targets,
        today,
    })
}

/// 写备份偏好（整份替换：目标、保留份数、自动两条、提示条是否被拒过）。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_config_write(
    data: State<'_, AppData>,
    config: BackupConfig,
) -> Result<BackupConfig, ApiError> {
    data.with_store(|store: &mut Store| {
        store.set_backup_config(&config)?;
        Ok(store.backup_config()?)
    })
}

/// 立即备份到所有目标。**逐目标成败**：一个盘写不进去不影响别的盘。
#[tauri::command(rename_all = "snake_case")]
pub fn backup_now(
    data: State<'_, AppData>,
    tz_offset_minutes: i32,
) -> Result<BackupReport, ApiError> {
    let config = data.with_store(|store: &mut Store| Ok(store.backup_config()?))?;
    let data_dir = data
        .db_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    let request = BackupRequest {
        data_dir,
        targets: config.targets,
        keep: config.keep,
        tz_offset_minutes,
        device: device_name(),
    };
    data.with_store(|store: &mut Store| Ok(store.backup_now(&request)?))
}

// ── 盘符探测（Windows） ─────────────────────────────────────────────────────

/// 列出现在能看到的盘（含可移动盘）。非 Windows 返回空列表——那边还没有发布形态。
#[cfg(windows)]
fn list_volumes(data_dir: &Path) -> Vec<VolumeDto> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = unsafe { GetLogicalDrives() };
    let data_volume = crate::volume::volume_id_for(data_dir);
    let mut out = Vec::new();
    for index in 0..26u32 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let root = format!("{}:\\", (b'A' + index as u8) as char);
        let Some(info) = crate::volume::volume_info(&root) else { continue };
        let (free_bytes, total_bytes) = crate::volume::free_space(&root);
        out.push(VolumeDto {
            holds_data: !data_volume.is_empty() && info.volume_id == data_volume,
            root,
            label: info.label,
            volume_id: info.volume_id,
            removable: info.removable,
            free_bytes,
            total_bytes,
        });
    }
    out
}

#[cfg(not(windows))]
fn list_volumes(_data_dir: &Path) -> Vec<VolumeDto> {
    Vec::new()
}
