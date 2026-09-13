//! 从备份恢复（一）：**恢复前先看清楚**——列得出哪些备份、这一份体不体检通过、会丢多少。
//!
//! # 三条纪律（整套恢复功能的总纲，换库那一步见 [`super::restore_swap`]）
//!
//! 1. **先体检，再动手**：包对不上就明说"这份不完整"，**绝不硬恢复**
//!    （[`Store::preview_restore`] 给出结论与"会退回多少天 / 少多少字"）；
//! 2. **换库前先留底**：当前库（连同 `-wal` / `-shm`）整体搬进留底目录——**改名而不是删**，
//!    坏库也留证。只搬 `.db` 会把不一致的 WAL 留给新库，那比坏库更难查；
//! 3. **失败必须能回到原状**：新库放下之后当场再体检一次，不过就把它扔掉、把留底搬回来。
//!
//! 全套只做**文件层**的动作（纯标准库 + 只读探针），关库与重启由壳负责——
//! 核心不知道"谁正开着这个库"，也不该知道。
//!
//! 恢复源有两种：**备份包目录**（认里面的 `manifest.json`）与**单个库文件**
//! （命令行救援 / 作者手选来的那份）。两种都先体检，不给"看着像备份"的东西放行。
//!
//! # 文件划分
//!
//! - 本文件：体检预览（读得多、写得少，界面每次选中都要用）；
//! - [`super::restore_packages`]：列备份包（列表只用清单，不体检——体检要复制整份库）；
//! - [`super::restore_swap`]：真正换库（最危险的十几步，单独一处便于盯）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::backup::{last_write_at, read_manifest, work_stamps, BackupVerify, Probe};
use super::restore_packages::dir_bytes;
use super::restore_swap::{same_file, KEEP_FOLDER};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::version;

/// 恢复前的体检结论 + "会退回多少"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePreview {
    pub source: String,
    /// `package`（备份包）或 `database`（单个库文件）
    pub kind: String,
    pub verify: BackupVerify,
    /// 这份备份是什么时候做的 / 最后写到哪（毫秒）
    pub created_at: i64,
    pub device: String,
    pub engine_version: String,
    pub data_format_version: u32,
    pub source_last_write_at: i64,
    pub source_works: i64,
    pub source_chapters: i64,
    pub source_words: i64,
    pub source_bytes: u64,
    /// 现在这个库里最后写到哪 / 有多少（用来算"会丢多少"）
    pub live_last_write_at: i64,
    pub live_works: i64,
    pub live_words: i64,
    /// 会退回几天 / 大约少多少字（都取不出来的部分是 0）
    pub lost_days: i64,
    pub lost_words: i64,
    /// 选中的就是现在正在用的那个库（不能拿它恢复它自己）
    pub is_live_database: bool,
    /// 能不能恢复：体检通过、且不是"自己恢复自己"
    pub can_restore: bool,
    /// 换库时原库会留底到哪儿（给作者看的路径）
    pub keep_dir: String,
}

/// 体检结论 + 规模账（包与裸库两条路的共同产物）。
struct Inspected {
    kind: &'static str,
    verify: BackupVerify,
    created_at: i64,
    device: String,
    engine_version: String,
    data_format_version: u32,
    last_write_at: i64,
    works: i64,
    chapters: i64,
    words: i64,
    bytes: u64,
}

/// 看一份来源：备份包走清单 + 逐项体检；裸库走只读探针。
fn inspect(source: &Path, verifier: &Store) -> Result<Inspected> {
    if source.is_dir() {
        let Some(manifest) = read_manifest(source) else {
            return Err(source_invalid(source));
        };
        let mut problems = verifier.verify_backup(source).problems;
        if manifest.data_format_version > version::DATA_FORMAT_VERSION {
            problems.push(format!(
                "这份备份来自更新的研墨（数据格式 v{}），当前引擎只到 v{}",
                manifest.data_format_version,
                version::DATA_FORMAT_VERSION
            ));
        }
        let (mut chapters, mut words) = (0, 0);
        for work in &manifest.works {
            chapters += work.chapters;
            words += work.word_count;
        }
        let ok = problems.is_empty();
        return Ok(Inspected {
            kind: "package",
            verify: BackupVerify { ok, problems },
            created_at: manifest.created_at,
            device: manifest.device,
            engine_version: manifest.engine_version,
            data_format_version: manifest.data_format_version,
            last_write_at: manifest.db_last_write_at,
            works: manifest.works.len() as i64,
            chapters,
            words,
            bytes: dir_bytes(source),
        });
    }
    if source.is_file() {
        return Ok(inspect_database(source));
    }
    Err(source_invalid(source))
}

/// 裸库（不是备份包）：复制一份打开体检，再读它的规模账。
fn inspect_database(db: &Path) -> Inspected {
    let mut problems = Vec::new();
    let mut stats = Inspected {
        kind: "database",
        verify: BackupVerify { ok: false, problems: Vec::new() },
        created_at: modified_millis(db),
        device: String::new(),
        engine_version: String::new(),
        data_format_version: version::DATA_FORMAT_VERSION,
        last_write_at: 0,
        works: 0,
        chapters: 0,
        words: 0,
        bytes: std::fs::metadata(db).map(|meta| meta.len()).unwrap_or(0),
    };
    match Probe::open(db) {
        Err(problem) => problems.push(problem),
        Ok(probe) => {
            match crate::db::quick_check(probe.conn()) {
                Ok(verdict) if verdict == "ok" => {}
                Ok(verdict) => problems.push(format!("这个库结构有问题：{verdict}")),
                Err(error) => problems.push(format!("这个库体检失败：{error}")),
            }
            match work_stamps(probe.conn()) {
                Err(error) => problems.push(format!("这个库读不出书目：{error}")),
                Ok(rows) => {
                    stats.works = rows.len() as i64;
                    for row in &rows {
                        stats.chapters += row.2;
                        stats.words += row.3;
                    }
                }
            }
            stats.last_write_at = last_write_at(probe.conn()).unwrap_or(0);
            // 裸库记的是**库结构版本**（`user_version`），跟清单里的"数据格式版本"不是一回事：
            // 它要跟引擎的迁移表末位比，不能拿去比 `DATA_FORMAT_VERSION`（会误报"来自更新的研墨"）。
            let schema = crate::db::migrations::user_version(probe.conn()).unwrap_or(0);
            let supported = crate::db::migrations::schema_version();
            if schema > supported {
                problems.push(format!(
                    "这个库来自更新的研墨（库结构 v{schema}），当前引擎只到 v{supported}"
                ));
            }
        }
    }
    stats.verify = BackupVerify { ok: problems.is_empty(), problems };
    stats
}

impl Store {
    /// 恢复前先看清楚：这份来源体不体检通过、换上去会退回多少天 / 少多少字。
    ///
    /// **不动任何文件**：体检跑在临时副本上（见 [`Probe`]），活库只做只读查询。
    pub fn preview_restore(&self, source: &Path) -> Result<RestorePreview> {
        let live_rows = work_stamps(&self.conn)?;
        let live_works = live_rows.len() as i64;
        let live_chapters: i64 = live_rows.iter().map(|row| row.2).sum();
        let live_words: i64 = live_rows.iter().map(|row| row.3).sum();
        let live_last_write_at = last_write_at(&self.conn)?;

        let db_path = self.conn.path().map(PathBuf::from);
        let is_live_database = db_path
            .as_deref()
            .map(|live| same_file(live, source))
            .unwrap_or(false);
        let keep_dir = db_path
            .as_deref()
            .and_then(Path::parent)
            .map(|dir| dir.join(KEEP_FOLDER).to_string_lossy().to_string())
            .unwrap_or_default();

        let inspected = if is_live_database {
            // 它正被我们开着用——**不是坏了，只是不该当来源**。这里不复制、不体检：
            // 复制一个开着 WAL 的库只会拿到没归并的主文件，那是我们自己制造的"假损坏"。
            Inspected {
                kind: "database",
                verify: BackupVerify { ok: true, problems: Vec::new() },
                created_at: modified_millis(source),
                device: String::new(),
                engine_version: version::engine_version().to_string(),
                data_format_version: version::DATA_FORMAT_VERSION,
                last_write_at: live_last_write_at,
                works: live_works,
                chapters: live_chapters,
                words: live_words,
                bytes: std::fs::metadata(source).map(|meta| meta.len()).unwrap_or(0),
            }
        } else {
            inspect(source, self)?
        };

        // 会退回几天：活库最后写入 - 这份备份的最后写入（正数才算数，向上取整到天）
        let delta = live_last_write_at - inspected.last_write_at;
        let lost_days = if delta > 0 { (delta + 86_399_999) / 86_400_000 } else { 0 };
        let lost_words = (live_words - inspected.words).max(0);

        Ok(RestorePreview {
            source: source.to_string_lossy().to_string(),
            kind: inspected.kind.to_string(),
            can_restore: inspected.verify.ok && !is_live_database,
            verify: inspected.verify,
            created_at: inspected.created_at,
            device: inspected.device,
            engine_version: inspected.engine_version,
            data_format_version: inspected.data_format_version,
            source_last_write_at: inspected.last_write_at,
            source_works: inspected.works,
            source_chapters: inspected.chapters,
            source_words: inspected.words,
            source_bytes: inspected.bytes,
            live_last_write_at,
            live_works,
            live_words,
            lost_days,
            lost_words,
            is_live_database,
            keep_dir,
        })
    }
}

/// 选中的位置既不是备份包、也不是库文件。
pub(super) fn source_invalid(path: &Path) -> Error {
    Error::invalid_with(
        codes::BACKUP_RESTORE_SOURCE_INVALID,
        [("path", path.display().to_string())],
    )
}

/// 文件的最后修改时间（裸库没有清单，"这份是什么时候的"只能看它）。
fn modified_millis(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
