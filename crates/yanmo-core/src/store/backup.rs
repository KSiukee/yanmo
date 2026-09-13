//! 多处备份：把库的一致性快照 + 成稿导出写到作者指定的几个位置。
//!
//! # 三条不能省的纪律
//!
//! 1. **快照必须是一致性快照**：绝不能直接复制正在写的 `yanmo.db`（连带 `-wal` 一起复制
//!    会拿到不一致的状态，**而且不报错**——这是最坏的一种坏，因为作者以为有备份）。
//!    这里用 `VACUUM INTO`：它在事务里读，写出来的是一个完整、自洽的单文件库。
//! 2. **每次备份是一个有记录的事件**：包里自带 `manifest.json`（时间 / 来源机器 / 库格式版本 /
//!    逐书章节数与内容指纹 / **上一份的指纹**＝链式）；写完**读回体检**（快照 `quick_check`
//!    ＋逐书比对）通过才算成功；结果写进**备份账本**。
//! 3. **账本存在数据库之外**（数据目录里的 `backup-ledger.json`）：它最需要被读到的时刻，
//!    正是"库打不开/坏了"的时候。写成原子写，并把最近一份副本放进每个备份包。
//!
//! # 诚实边界
//!
//! 指纹用的是核心自带的 [`crate::text::content_hash`]（FNV-1a 64 位）。它能发现
//! **备份不完整、文件被意外改坏、链路断了一环**这类问题；它**不是**防篡改的密码学证明
//! （要防篡改得签名，那是另一件事）。别把它说成"校验和证明"。
//!
//! 另外：备份**失败绝不阻断写作与关窗**——每个目标各自成败，逐目标报告。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{ExportFormat, Store};
use crate::atomic::write_atomic;
use crate::error::{Error, Result};
use crate::text;
use crate::time::{local_date, local_stamp, now_millis};
use crate::version;

/// 备份包目录名的前缀——**只认自己写出来的包**（滚动清理时绝不误删别人的东西）。
pub const PACKAGE_PREFIX: &str = "研墨备份-";
/// 包里的清单文件名（也是"这是不是一个备份包"的判据）。
pub const MANIFEST_FILE: &str = "manifest.json";
/// 包里的快照文件名。
pub const SNAPSHOT_FILE: &str = "yanmo.db";
/// 账本文件名（数据目录里，库之外）。
pub const LEDGER_FILE: &str = "backup-ledger.json";
/// 清单的 kind 取值（防止把别家的 manifest.json 当成我们的）。
pub const MANIFEST_KIND: &str = "yanmo-backup";

/// 一个备份目标（作者勾的一处落点）。
///
/// `volume_id` / `volume_label` 是**那块盘**的标识（Windows 卷序列号 + 卷标），
/// 由壳探测后传进来：**不能只记盘符**——换个 USB 口 `E:` 就变成 `F:` 了。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupTarget {
    /// 目标目录（作者指定的那个文件夹）
    pub path: String,
    #[serde(default)]
    pub volume_id: String,
    #[serde(default)]
    pub volume_label: String,
    /// 可移动盘（U 盘 / 移动硬盘）——写完之后提示"可以安全拔出"
    #[serde(default)]
    pub removable: bool,
}

/// 备份偏好：写到哪几处、留几份、什么时候自动做。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupConfig {
    pub targets: Vec<BackupTarget>,
    /// 每个目标保留最近几份**成功的**备份
    pub keep: usize,
    /// 每日首次启动时自动做一次
    pub auto_on_start: bool,
    /// 正常关窗时异步做一次（不拖慢退出）
    pub auto_on_close: bool,
    /// 作者拒绝过那条"插个盘吧"的小条——**拒过就不再自动弹**（设置页仍常驻一行建议）
    #[serde(default)]
    pub tip_dismissed: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        // 默认一份目标都不勾（只推荐、不预勾）；自动两条默认开——没配目标时它们什么都不做
        Self {
            targets: Vec::new(),
            keep: 7,
            auto_on_start: true,
            auto_on_close: true,
            tip_dismissed: false,
        }
    }
}

/// 逐书的规模账（清单里记一份，体检时拿快照里的数对一遍）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkStamp {
    pub id: i64,
    pub title: String,
    pub chapters: i64,
    pub word_count: i64,
    pub chars_no_punct: i64,
    /// 成稿导出里属于这本书的文件（相对包目录，按顺序），指纹就是按这个顺序算的
    pub files: Vec<String>,
    /// 成稿内容的指纹
    pub fingerprint: String,
}

/// 一份备份的清单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub kind: String,
    pub created_at: i64,
    /// 目录名里的时间戳（本地时间；与目录名一致，便于人对着看）
    pub stamp: String,
    pub engine_version: String,
    pub data_format_version: u32,
    /// 来源机器名（多台机器共用一个备份盘时，一眼看出这份是谁写的）
    pub device: String,
    pub snapshot_file: String,
    pub snapshot_bytes: u64,
    /// 库最后一次写入的时间（恢复时算"会丢多少天"要用）
    pub db_last_write_at: i64,
    pub works: Vec<WorkStamp>,
    /// 上一次成功备份的指纹（链式：断了一环说明中间那份没了）
    pub previous_fingerprint: Option<String>,
    /// 本份的指纹（含链上一条）
    pub fingerprint: String,
}

/// 一个目标的备份结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetOutcome {
    pub path: String,
    pub volume_label: String,
    /// `written` / `skipped` / `failed`
    pub status: String,
    /// 给人话的原因（成功时为空）
    pub reason: String,
    /// 备份包目录（成功时）
    pub package: String,
    pub bytes: u64,
    pub kept: usize,
    pub removed: usize,
    pub fingerprint: String,
    pub at: i64,
}

/// 一次备份的总账。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupReport {
    pub at: i64,
    pub stamp: String,
    pub outcomes: Vec<TargetOutcome>,
}

impl BackupReport {
    pub fn succeeded(&self) -> usize {
        self.outcomes.iter().filter(|o| o.status == "written").count()
    }
    pub fn skipped(&self) -> usize {
        self.outcomes.iter().filter(|o| o.status == "skipped").count()
    }
    pub fn failed(&self) -> usize {
        self.outcomes.iter().filter(|o| o.status == "failed").count()
    }
}

/// 账本里的一条：**每次尝试都记**（成功、跳过、失败都记，空档不用猜）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub at: i64,
    /// 本地日期 `2026-09-13`（"缺了哪几天"按它算）
    pub date: String,
    pub stamp: String,
    pub target_path: String,
    pub volume_label: String,
    pub status: String,
    pub reason: String,
    pub fingerprint: String,
}

/// 备份账本（库之外的 JSON）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupLedger {
    pub version: u32,
    pub updated_at: i64,
    /// 按时间从旧到新追加；只保留最近 [`LEDGER_MAX_ENTRIES`] 条
    pub entries: Vec<LedgerEntry>,
}

/// 账本最多留多少条（够看"最近这些天有没有做"，又不至于无限长）。
pub const LEDGER_MAX_ENTRIES: usize = 500;

impl BackupLedger {
    /// 某个目标最后一次**成功**的那条。
    pub fn last_success(&self, target_path: &str) -> Option<&LedgerEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.target_path == target_path && e.status == "written")
    }

    /// 某个目标最后一次尝试（不管成败）。
    pub fn last_attempt(&self, target_path: &str) -> Option<&LedgerEntry> {
        self.entries.iter().rev().find(|e| e.target_path == target_path)
    }

    /// 某个目标最近的失败原因（给设置页显示"上回为什么没成"）。
    pub fn last_failure(&self, target_path: &str) -> Option<&LedgerEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.target_path == target_path && e.status != "written")
    }
}

/// 一次备份请求。
#[derive(Debug, Clone)]
pub struct BackupRequest {
    /// 数据目录（账本与暂存区都放这儿）
    pub data_dir: PathBuf,
    pub targets: Vec<BackupTarget>,
    pub keep: usize,
    /// 本地时区相对 UTC 的分钟偏移（核心不猜时区，由界面传进来）
    pub tz_offset_minutes: i32,
    /// 来源机器名（多机共用备份盘时用来分辨）
    pub device: String,
}

/// 读账本（**只读文件，不碰库**——库坏了也要能读）。
pub fn read_ledger(data_dir: &Path) -> BackupLedger {
    let path = data_dir.join(LEDGER_FILE);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<BackupLedger>(&text).ok())
        .unwrap_or_default()
}

/// 读一份备份包的清单（不是我们的包就返回 `None`）。
pub fn read_manifest(package: &Path) -> Option<BackupManifest> {
    let text = std::fs::read_to_string(package.join(MANIFEST_FILE)).ok()?;
    let manifest: BackupManifest = serde_json::from_str(&text).ok()?;
    (manifest.kind == MANIFEST_KIND).then_some(manifest)
}

/// 体检结论（本轮的"读回体检"用它；下一步的"恢复前先体检"也用它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupVerify {
    pub ok: bool,
    /// 一条条问题（空了就是全过）
    pub problems: Vec<String>,
}

/// 逐书的规模账（活库与快照用**同一条 SQL** 取，否则体检就成了两套口径）。
///
/// `pub(super)`：从备份恢复那边也要用它读"这份备份里有什么"——两处必须同一口径。
pub(super) fn work_stamps(conn: &Connection) -> Result<Vec<(i64, String, i64, i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT w.id, w.title,
                (SELECT COUNT(*) FROM nodes n
                  WHERE n.work_id = w.id AND n.deleted_at IS NULL AND n.node_kind = 'chapter'),
                (SELECT COALESCE(SUM(n.word_count), 0) FROM nodes n
                  WHERE n.work_id = w.id AND n.deleted_at IS NULL),
                (SELECT COALESCE(SUM(n.chars_no_punct), 0) FROM nodes n
                  WHERE n.work_id = w.id AND n.deleted_at IS NULL)
           FROM works w
          WHERE w.deleted_at IS NULL
          ORDER BY w.id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 库最后一次写入时间（恢复时算"会丢多少天"）。
pub(super) fn last_write_at(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row("SELECT COALESCE(MAX(updated_at), 0) FROM nodes", [], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

impl Store {
    /// 读备份偏好（没设过就是默认：不勾任何目标、保留 7 份、自动两条开）。
    pub fn backup_config(&self) -> Result<BackupConfig> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = 'backup.config'", [], |r| r.get(0))
            .optional()?;
        Ok(raw
            .and_then(|json| serde_json::from_str::<BackupConfig>(&json).ok())
            .unwrap_or_default())
    }

    /// 写备份偏好（整份替换——它是一份设置，不是稀疏偏好）。
    pub fn set_backup_config(&mut self, config: &BackupConfig) -> Result<()> {
        let json = serde_json::to_string(config).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('backup.config', ?1, ?2)",
            params![json, now_millis()],
        )?;
        Ok(())
    }

    /// 立刻备份到所有目标。**逐目标成败**，一个目标失败不影响别的目标。
    ///
    /// 快照只做一次（所有目标共用同一份），这样多目标之间是**同一时刻**的库。
    pub fn backup_now(&mut self, req: &BackupRequest) -> Result<BackupReport> {
        let at = now_millis();
        let stamp = local_stamp(at, req.tz_offset_minutes);
        let date = local_date(at, req.tz_offset_minutes);
        let mut ledger = read_ledger(&req.data_dir);

        // ① 先做一份一致性快照到暂存区，并把成稿、清单一起准备好
        let staging = req.data_dir.join(format!(".backup-staging-{stamp}"));
        let prepared = self.prepare_package(&staging, &stamp, at, req, &ledger);
        let (manifest, snapshot_bytes) = match prepared {
            Ok(v) => v,
            Err(error) => {
                // 快照都做不出来（库层面的问题）：所有目标一起记失败，原因如实写
                let reason = format!("{error}");
                let mut outcomes = Vec::new();
                for target in &req.targets {
                    outcomes.push(outcome(target, "failed", &reason, "", 0, 0, 0, "", at));
                }
                for target in &req.targets {
                    ledger.entries.push(entry(target, &date, &stamp, "failed", &reason, "", at));
                }
                ledger.updated_at = at;
                write_ledger(&req.data_dir, &ledger);
                return Ok(BackupReport { at, stamp, outcomes });
            }
        };

        // ② 逐个目标写包 + 体检 + 滚动清理
        let mut outcomes = Vec::new();
        for target in &req.targets {
            let (outcome, ledger_entry) =
                self.write_to_target(target, &staging, &manifest, snapshot_bytes, &date, &stamp, req.keep, at);
            ledger.entries.push(ledger_entry);
            outcomes.push(outcome);
        }

        // ③ 账本回写（原子写）+ 收尾
        ledger.updated_at = at;
        ledger.version = 1;
        let overflow = ledger.entries.len().saturating_sub(LEDGER_MAX_ENTRIES);
        if overflow > 0 {
            ledger.entries.drain(0..overflow);
        }
        write_ledger(&req.data_dir, &ledger);
        std::fs::remove_dir_all(&staging).ok(); // 暂存区用完就删（失败也无所谓）
        Ok(BackupReport { at, stamp, outcomes })
    }

    /// 把"这一份备份"准备好：快照 + 成稿导出 + 清单（都放暂存区，之后逐个目标复制）。
    fn prepare_package(
        &self,
        staging: &Path,
        stamp: &str,
        at: i64,
        req: &BackupRequest,
        ledger: &BackupLedger,
    ) -> Result<(BackupManifest, u64)> {
        // 暂存区建不出来（被文件占住 / 没权限）与快照失败是同一类事：**这份备份做不出来**，
        // 报错要往"快照"这条线上说，别丢一个裸 IO 错误给作者。
        std::fs::create_dir_all(staging).map_err(|e| {
            Error::invalid_with(
                crate::error::codes::BACKUP_SNAPSHOT_FAILED,
                [("detail", format!("暂存区建不出来：{e}"))],
            )
        })?;
        // 一致性快照：VACUUM INTO 写出来的是自洽的单文件库（目标文件必须不存在）
        let snapshot = staging.join(SNAPSHOT_FILE);
        let snapshot_path = snapshot.to_string_lossy().to_string();
        self.conn
            .execute("VACUUM INTO ?1", params![snapshot_path])
            .map_err(|e| Error::invalid_with(crate::error::codes::BACKUP_SNAPSHOT_FAILED, [("detail", e.to_string())]))?;
        let snapshot_bytes = std::fs::metadata(&snapshot).map(|m| m.len()).unwrap_or(0);

        // 成稿导出（不装研墨也能读的第二层保险）+ 逐书指纹
        let mut works = Vec::new();
        for (id, title, chapters, word_count, chars_no_punct) in work_stamps(&self.conn)? {
            let mut files = Vec::new();
            let mut combined = String::new();
            let dir_name = crate::atomic::safe_file_name(&title);
            for (format, sub) in [(ExportFormat::Text, ""), (ExportFormat::Json, "")] {
                for file in self.render_work(id, format)? {
                    let relative = if format == ExportFormat::Json {
                        format!("成稿/{sub}{}", file.relative_path)
                    } else {
                        format!("成稿/{dir_name}/{}", file.relative_path)
                    };
                    let path = staging.join(&relative);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, &file.content)?;
                    // 指纹只算文本成稿（人读的那一份）：够判断"内容是不是这一版"
                    if format == ExportFormat::Text {
                        combined.push_str(&String::from_utf8_lossy(&file.content));
                    }
                    files.push(relative.replace('\\', "/"));
                }
            }
            works.push(WorkStamp {
                id,
                title,
                chapters,
                word_count,
                chars_no_punct,
                files,
                fingerprint: text::content_hash(&combined),
            });
        }

        // 账本副本（库打不开时，从包里也能看到备份史）
        let ledger_bytes = serde_json::to_vec_pretty(ledger).unwrap_or_default();
        std::fs::write(staging.join(LEDGER_FILE), &ledger_bytes)?;

        // 链式指纹：把"这一份的实质内容 + 上一份指纹"一起摘要
        let mut canonical = String::new();
        canonical.push_str(&format!("{stamp}|{}|{snapshot_bytes}|{}|", req.device, version::DATA_FORMAT_VERSION));
        for work in &works {
            canonical.push_str(&format!("{}:{}:{}:{}|", work.id, work.chapters, work.word_count, work.fingerprint));
        }
        let previous_fingerprint = req
            .targets
            .iter()
            .find_map(|t| ledger.last_success(&t.path).map(|e| e.fingerprint.clone()));
        if let Some(previous) = &previous_fingerprint {
            canonical.push_str(previous);
        }
        let fingerprint = text::content_hash(&canonical);

        let manifest = BackupManifest {
            kind: MANIFEST_KIND.to_string(),
            created_at: at,
            stamp: stamp.to_string(),
            engine_version: version::engine_version().to_string(),
            data_format_version: version::DATA_FORMAT_VERSION,
            device: req.device.clone(),
            snapshot_file: SNAPSHOT_FILE.to_string(),
            snapshot_bytes,
            db_last_write_at: last_write_at(&self.conn)?,
            works,
            previous_fingerprint,
            fingerprint,
        };
        write_atomic(&staging.join(MANIFEST_FILE), &serde_json::to_vec_pretty(&manifest).unwrap_or_default())?;
        Ok((manifest, snapshot_bytes))
    }

    /// 把暂存好的这一份复制到某个目标：写包 → 体检 → 滚动清理。
    fn write_to_target(
        &self,
        target: &BackupTarget,
        staging: &Path,
        manifest: &BackupManifest,
        snapshot_bytes: u64,
        date: &str,
        stamp: &str,
        keep: usize,
        at: i64,
    ) -> (TargetOutcome, LedgerEntry) {
        let root = PathBuf::from(&target.path);
        // 目标不可达：**跳过并记账**，不做隐式补做。
        // 原因要分清——"盘不在"是常态（拔了盘），"写不进去"是真故障（权限/路径被占），
        // 一句"盘没插？"糊过去会让人查错方向。
        if let Err(error) = std::fs::create_dir_all(&root) {
            let reason = if volume_root_exists(&target.path) {
                format!("写不进去：{error}")
            } else {
                "盘不在（没插？）".to_string()
            };
            return (
                outcome(target, "skipped", &reason, "", 0, 0, 0, "", at),
                entry(target, date, stamp, "skipped", &reason, "", at),
            );
        }
        let package = root.join(format!("{PACKAGE_PREFIX}{stamp}"));
        // 同名包已存在（同一分钟内做了两次）：换一个带序号的目录，绝不覆盖已有备份
        let package = unique_package(package);

        let copied = copy_tree(staging, &package);
        if let Err(error) = copied {
            let reason = format!("写备份失败：{error}");
            std::fs::remove_dir_all(&package).ok();
            return (
                outcome(target, "failed", &reason, "", 0, 0, 0, "", at),
                entry(target, date, stamp, "failed", &reason, "", at),
            );
        }

        // 读回体检：通不过就不算备份（验不过的包删掉，免得被当成一份"备份"）
        let verify = self.verify_backup(&package);
        if !verify.ok {
            let reason = format!("读回体检没通过：{}", verify.problems.join("；"));
            std::fs::remove_dir_all(&package).ok();
            return (
                outcome(target, "failed", &reason, "", 0, 0, 0, "", at),
                entry(target, date, stamp, "failed", &reason, "", at),
            );
        }

        let removed = prune_packages(&root, keep);
        (
            outcome(
                target,
                "written",
                "",
                &package.to_string_lossy(),
                snapshot_bytes,
                0,
                removed,
                &manifest.fingerprint,
                at,
            ),
            entry(target, date, stamp, "written", "", &manifest.fingerprint, at),
        )
    }

    /// 体检一份备份包：清单 → 快照能不能打开且结构完好 → 逐书比对 → 成稿文件还在且指纹对得上。
    pub fn verify_backup(&self, package: &Path) -> BackupVerify {
        let mut problems = Vec::new();
        let Some(manifest) = read_manifest(package) else {
            return BackupVerify { ok: false, problems: vec![format!("{MANIFEST_FILE} 读不出来（不是研墨的备份包？）")] };
        };

        let snapshot = package.join(&manifest.snapshot_file);
        if !snapshot.is_file() {
            problems.push(format!("找不到快照：{}", manifest.snapshot_file));
        } else {
            match Probe::open(&snapshot) {
                Err(problem) => problems.push(problem),
                Ok(probe) => {
                    match crate::db::quick_check(probe.conn()) {
                        Ok(verdict) if verdict == "ok" => {}
                        Ok(verdict) => problems.push(format!("快照结构有问题：{verdict}")),
                        Err(error) => problems.push(format!("快照体检失败：{error}")),
                    }
                    // 逐书比对：章节数 + 字数（与清单里记的对得上，才说明这份快照是"完整的")
                    match work_stamps(probe.conn()) {
                        Err(error) => problems.push(format!("快照读不出书目：{error}")),
                        Ok(rows) => {
                            if rows.len() != manifest.works.len() {
                                problems.push(format!(
                                    "书目对不上：快照 {} 本，清单 {} 本",
                                    rows.len(),
                                    manifest.works.len()
                                ));
                            }
                            for (id, title, chapters, word_count, chars_no_punct) in rows {
                                match manifest.works.iter().find(|w| w.id == id) {
                                    None => problems.push(format!("快照里多出一本：{title}（#{id}）")),
                                    Some(stamp) => {
                                        if stamp.chapters != chapters
                                            || stamp.word_count != word_count
                                            || stamp.chars_no_punct != chars_no_punct
                                        {
                                            problems.push(format!(
                                                "《{title}》对不上：清单 {} 章/{} 字，快照 {} 章/{} 字",
                                                stamp.chapters, stamp.word_count, chapters, word_count
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 成稿文件：在、指纹对得上（文件被改坏/少了一个，这里就会说话）
        for work in &manifest.works {
            let mut combined = String::new();
            for relative in &work.files {
                match std::fs::read_to_string(package.join(relative)) {
                    Err(error) => problems.push(format!("《{}》的成稿读不到（{relative}）：{error}", work.title)),
                    Ok(content) => {
                        if relative.ends_with(".txt") {
                            combined.push_str(&content);
                        }
                    }
                }
            }
            if !combined.is_empty() && text::content_hash(&combined) != work.fingerprint {
                problems.push(format!("《{}》的成稿内容与清单不符", work.title));
            }
        }

        BackupVerify { ok: problems.is_empty(), problems }
    }
}

/// 体检用的**临时副本**：把库复制到临时文件再打开（绝不改动原库）。
///
/// 两条理由：
/// ① FTS5 的索引校验需要写权限，只读连接会报 "attempt to write a readonly database"；
/// ② 备份包是"写一次、之后只读"的东西，不该因为体检而改动（连 `-wal`/`-shm` 都不该出现）。
/// 复制本身也是一道读校验：整份文件读不出来的话，这里就失败了。
///
/// 临时名必须**每次唯一**：并行体检（或同一份包体检两次）时共用一个名字会互相踩——
/// 一个刚删掉另一个正要打开的文件。进程号 + 纳秒 + 进程内序号，三样一起才够。
pub(super) struct Probe {
    /// `None` 表示连接已经收了。Drop 里**先收连接再删文件**：Windows 上占着句柄删不掉。
    conn: Option<Connection>,
    path: PathBuf,
}

impl Probe {
    pub(super) fn open(db: &Path) -> std::result::Result<Self, String> {
        static PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("yanmo-verify-{}-{nanos}-{seq}.db", std::process::id()));
        std::fs::copy(db, &path).map_err(|error| format!("快照读不出来：{error}"))?;
        match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE) {
            Ok(conn) => Ok(Self { conn: Some(conn), path }),
            Err(error) => {
                cleanup_probe(&path);
                Err(format!("快照打不开：{error}"))
            }
        }
    }

    pub(super) fn conn(&self) -> &Connection {
        self.conn.as_ref().expect("体检副本的连接还没到收的时候")
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.conn.take();
        cleanup_probe(&self.path);
    }
}

/// 收掉临时副本连同它可能生成的 `-wal` / `-shm`。
fn cleanup_probe(probe: &Path) {
    std::fs::remove_file(probe).ok();
    std::fs::remove_file(probe.with_extension("db-wal")).ok();
    std::fs::remove_file(probe.with_extension("db-shm")).ok();
}

fn outcome(
    target: &BackupTarget,
    status: &str,
    reason: &str,
    package: &str,
    bytes: u64,
    kept: usize,
    removed: usize,
    fingerprint: &str,
    at: i64,
) -> TargetOutcome {
    TargetOutcome {
        path: target.path.clone(),
        volume_label: target.volume_label.clone(),
        status: status.to_string(),
        reason: reason.to_string(),
        package: package.to_string(),
        bytes,
        kept,
        removed,
        fingerprint: fingerprint.to_string(),
        at,
    }
}

fn entry(
    target: &BackupTarget,
    date: &str,
    stamp: &str,
    status: &str,
    reason: &str,
    fingerprint: &str,
    at: i64,
) -> LedgerEntry {
    LedgerEntry {
        at,
        date: date.to_string(),
        stamp: stamp.to_string(),
        target_path: target.path.clone(),
        volume_label: target.volume_label.clone(),
        status: status.to_string(),
        reason: reason.to_string(),
        fingerprint: fingerprint.to_string(),
    }
}

/// 写账本（原子写：账本读坏了就没人知道备份做没做）。
fn write_ledger(data_dir: &Path, ledger: &BackupLedger) {
    let bytes = serde_json::to_vec_pretty(ledger).unwrap_or_default();
    write_atomic(&data_dir.join(LEDGER_FILE), &bytes).ok();
}

/// 目标路径所在盘的根在不在（区分"盘不在"与"写不进去"用）。
///
/// 一路往上找到最顶层那一级（盘根或相对路径的第一段），再问它在不在。
fn volume_root_exists(path: &str) -> bool {
    let mut root = PathBuf::from(path);
    while let Some(parent) = root.parent() {
        if parent.as_os_str().is_empty() {
            break;
        }
        root = parent.to_path_buf();
    }
    root.exists()
}

/// 同名包换一个带序号的目录（同一分钟里做两次也不覆盖）。
fn unique_package(base: PathBuf) -> PathBuf {
    if !base.exists() {
        return base;
    }
    for index in 2..100 {
        let candidate = base.with_file_name(format!(
            "{}-{index}",
            base.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    base
}

/// 复制整棵目录树（备份包不大，够用且直观）。
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for item in std::fs::read_dir(from)? {
        let item = item?;
        let target = to.join(item.file_name());
        if item.file_type()?.is_dir() {
            copy_tree(&item.path(), &target)?;
        } else {
            std::fs::copy(item.path(), &target)?;
        }
    }
    Ok(())
}

/// 滚动清理：**只删自己写出来的包**（目录名前缀 + 包里有我们的清单），保留最近 `keep` 份。
///
/// 返回删掉几份。别人的文件、半截的目录一律不碰——"清理"最忌讳误删。
fn prune_packages(root: &Path, keep: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut packages: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(PACKAGE_PREFIX))
                    .unwrap_or(false)
                && read_manifest(p).is_some()
        })
        .collect();
    // 名字里带 `YYYYMMDD-HHMM`，字面量排序就是时间排序
    packages.sort();
    let keep = keep.max(1);
    if packages.len() <= keep {
        return 0;
    }
    let mut removed = 0;
    for old in &packages[..packages.len() - keep] {
        if std::fs::remove_dir_all(old).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// 每个目标"最近一次成功是哪天 / 缺了哪几天"——设置页一眼看到空档（单位：天）。
pub fn gaps_for(ledger: &BackupLedger, target_path: &str, today: &str, days: u32) -> Vec<String> {
    let succeeded: Vec<&str> = ledger
        .entries
        .iter()
        .filter(|e| e.target_path == target_path && e.status == "written")
        .map(|e| e.date.as_str())
        .collect();
    // 从今天往回数 days 天，没成功记录的就是空档（日期字面量可比较，不必解析）
    let today_days = days_from_date(today);
    let mut gaps = Vec::new();
    for back in 0..days {
        let Some(day) = today_days.and_then(|d| date_from_days(d - i64::from(back))) else {
            break;
        };
        if !succeeded.contains(&day.as_str()) {
            gaps.push(day);
        }
    }
    gaps
}

/// `2026-09-13` → 1970 起的天数（只用于按天回退，解析失败就返回 None）。
fn days_from_date(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // 与 time::civil_from_days 互逆的 days_from_civil
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// 1970 起的天数 → `2026-09-13`。
fn date_from_days(days: i64) -> Option<String> {
    let (y, m, d, _, _, _) = crate::time::local_parts(days * 86_400_000, 0);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// 目标列表里有没有"异盘"目标（插盘提醒的判据之一：一个都没有就该提醒）。
pub fn has_other_volume(config: &BackupConfig, data_volume_id: &str) -> bool {
    config
        .targets
        .iter()
        .any(|t| !t.volume_id.is_empty() && t.volume_id != data_volume_id)
}

/// 按卷标识聚合的账本摘要（设置页要"每个目标最后一次成功 / 空档"）。
pub fn ledger_summary(ledger: &BackupLedger) -> Vec<(String, String, Option<String>, Option<String>, i64)> {
    let mut map: BTreeMap<String, (String, Option<String>, Option<String>, i64)> = BTreeMap::new();
    for item in &ledger.entries {
        let slot = map
            .entry(item.target_path.clone())
            .or_insert((item.volume_label.clone(), None, None, 0));
        slot.3 += 1;
        if item.status == "written" {
            slot.1 = Some(item.date.clone());
        } else {
            slot.2 = Some(item.reason.clone());
        }
    }
    map.into_iter()
        .map(|(path, (label, last_success, last_reason, count))| (path, label, last_success, last_reason, count))
        .collect()
}
