//! 运行期数据权威句柄：**数据目录由壳解析，前端没有传路径的入口**。
//!
//! 单一真相源在这里落成三条硬约束：
//!
//! 1. 目录来自系统 API（应用数据目录）+ 固定文件名，**不拼接任何外部输入**；
//! 2. 数据库由核心打开与迁移（`yanmo_core::store::Store`），壳只持有句柄；
//! 3. 命令层拿到的是本类型，**不是路径字符串**——绕开这条链路的入口不存在。
//!
//! 另外管两件与"别丢字"有关的事：**本次会话的登记**（用于崩溃检测）与
//! **退出闸门的开关**（界面就绪后才允许拦关窗，否则前端一出问题窗口就关不掉了）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use yanmo_core::location::{self, DataLocation, DirSource, Suggestion, Sources};
use yanmo_core::store::{Relocation, SessionReport, Store};

use crate::error::ApiError;
use crate::exitwatch::ExitWatch;

/// 逃生导出目录（关窗存不下去时，把手上这份正文原子写到这里）。
const ESCAPE_DIR: &str = "escape";
/// 系统答不上"文档在哪"时的导出落点：数据目录里的一个子目录。
const EXPORT_DIR: &str = "export";
// 库文件名与"文档/导出目录"的名字**不写在本文件**：图形界面与命令行救援入口是两个壳，
// 它们必须认同同一份约定（常量与选择规则都在 `yanmo_core::paths`）——
// 各写一份的结果是改一处漏一处，作者会以为稿子分家了。

/// 一次导出的结果（路径只报给界面看，界面拿到也改不了）。
pub struct ExportOutcome {
    pub dir: PathBuf,
    pub files: usize,
    pub removed: usize,
}

/// 清掉导出目录里这次不再需要的 txt / json，再收掉空目录。
///
/// 只动我们自己的两种后缀，且只在导出目录内——**作者往里放的东西一概不碰**。
fn prune_export(
    dir: &Path,
    files: &[yanmo_core::store::RenderedFile],
) -> Result<usize, ApiError> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let keep: std::collections::HashSet<String> = files
        .iter()
        .map(|file| file.relative_path.replace('/', std::path::MAIN_SEPARATOR_STR))
        .collect();
    let mut removed = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).map_err(|e| {
            ApiError::with(
                "shell.export_dir_unreadable",
                [("path", current.display().to_string())],
            )
            .caused_by(e)
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ours = path
                .extension()
                .is_some_and(|ext| ext == "txt" || ext == "json");
            if !ours {
                continue;
            }
            let relative = path
                .strip_prefix(dir)
                .map(|rest| rest.to_string_lossy().to_string())
                .unwrap_or_default();
            if !keep.contains(&relative) {
                std::fs::remove_file(&path).map_err(|e| {
                    ApiError::with(
                        "shell.export_file_remove_failed",
                        [("path", path.display().to_string())],
                    )
                    .caused_by(e)
                })?;
                removed += 1;
            }
        }
    }
    // 收掉空目录（自下而上，失败就当它还有用，不报错）
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    dirs.push(entry.path());
                    stack.push(entry.path());
                }
            }
        }
    }
    for path in dirs.into_iter().rev() {
        let _ = std::fs::remove_dir(&path);
    }
    Ok(removed)
}

/// 壳持有的数据句柄：核心的存储层句柄（唯一读写入口），加上它落在哪里。
///
/// `store` 是 `Option`：换库那一下必须**先把连接收干净**（[`AppData::close_store`]），
/// 换完再挂上（[`AppData::reopen_store`]）。没有这个"可关"的形态，就只能带着开着的
/// 连接去动库文件——那是拿作者的稿子冒险。
pub struct AppData {
    store: Mutex<Option<Store>>,
    /// 单实例守卫：**活着就占着这个数据目录**（见 [`crate::single`]）。
    ///
    /// `Option` 是为了换库重启那一条路：先把锁收出来交给后台线程，让它在重启前松开，
    /// 新进程马上就能要到同一把锁。
    instance: Mutex<Option<crate::single::InstanceLock>>,
    db_path: PathBuf,
    /// 导出根目录（**路径策略在壳**：界面既不选路径也不碰文件系统）
    export_dir: PathBuf,
    /// 位置记录文件：**"稿子放在哪"的权威记录**（见 [`yanmo_core::location`]）。
    pointer: PathBuf,
    /// 启动时定下来的数据目录 + 它是怎么定下来的。
    location: DataLocation,
    /// 首启推荐（界面拿它显示"建议放这里，因为…"）。
    suggestion: Option<Suggestion>,
    /// 定位置时问到的事实（"选新位置"要拿它算风险：同步盘 / 桌面 / 是不是可移动盘）。
    sources: Sources,
    /// 界面选好、等着确认的新位置。**路径只活在壳里**：界面拿不到，也递不进来。
    pending: Mutex<Option<PathBuf>>,
    session: SessionReport,
    exit_gate_armed: AtomicBool,
    exit_watch: ExitWatch,
}

/// 启动期定下来的事：数据目录在哪、位置记录写哪、要不要首启引导。
struct Plan {
    location: DataLocation,
    pointer: PathBuf,
    suggestion: Option<Suggestion>,
    sources: Sources,
}

impl AppData {
    /// 启动期调用一次：定数据目录 → 建目录 → 打开并迁移数据库 → 登记本次会话。
    ///
    /// 数据目录由 [`yanmo_core::location`] 定夺（位置记录优先 → 老位置认领 → 首启推荐）：
    /// 这里**不再靠"程序目录能不能写"去猜**——exe 被单独放到桌面时那种目录也可写，
    /// 猜出来的结果是稿库落在最容易误删、最容易被同步盘扫到的地方。
    ///
    /// 任何一步失败都直接返回错误，**绝不带病启动**（半个可用的数据层比不启动更危险）。
    pub fn open(app: &AppHandle) -> Result<Self, ApiError> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        // 「文档」「桌面」由**壳**问（走 Windows 已知文件夹，拿得到被 OneDrive 重定向后的真值）；
        // 命令行救援入口没有壳，那边用环境变量拼出来的近似值。
        let sources = Sources::from_env(exe_dir)
            .with_documents(app.path().document_dir().ok())
            .with_desktop(app.path().desktop_dir().ok());
        let pointer = location::pointer_path(&sources).ok_or_else(|| {
            ApiError::new("shell.data_dir_unavailable")
                .caused_by("no system data directory to keep the location record in")
        })?;
        let found = location::resolve(&sources).ok_or_else(|| {
            ApiError::new("shell.data_dir_unavailable")
                .caused_by("no data directory could be worked out")
        })?;
        let suggestion = location::suggest(&sources);
        // 导出放"文档/导出目录"：那是作者自己找得到的地方；系统答不上来就退回数据目录
        let export_dir = app
            .path()
            .document_dir()
            .map(|home| yanmo_core::paths::export_root(&home))
            .unwrap_or_else(|_| found.dir.join(EXPORT_DIR));
        let plan = Plan { location: found, pointer, suggestion, sources };
        let data = Self::open_at(plan, export_dir)?;
        // 老位置认领来的：**顺手把记录补上**，下次不用再认领一遍。
        // 写不进去不算启动失败——认领是确定性的（老位置就在那儿），下次还会落在同一个地方。
        if !data.location.source.is_first_run() && data.location.source != DirSource::Recorded {
            let _ = location::write_record(&data.pointer, &data.data_dir());
        }
        Ok(data)
    }

    /// **验收模式**：在指定目录上开数据层。
    ///
    /// 与正常启动的唯一区别：不读、也不写位置记录——验收模式造的是临时库，
    /// 绝不能因为跑一次验收就把作者真实稿库的位置记录改掉。
    pub fn open_for_acceptance(dir: &Path) -> Result<Self, ApiError> {
        let plan = Plan {
            location: DataLocation { dir: dir.to_path_buf(), source: DirSource::Recorded },
            // 验收模式不写记录：给一个"不会有人去写"的名字（ASCII，免得被当成界面文案），
            // 万一真被写了也只落在临时目录里
            pointer: dir.join("self-test-pointer-never-written.txt"),
            suggestion: None,
            sources: Sources::from_env(dir.to_path_buf()),
        };
        Self::open_at(plan, dir.join(EXPORT_DIR))
    }

    /// 目录由调用方给出——供测试直接驱动。
    #[cfg(test)]
    fn open_at_for_test(dir: &Path) -> Result<Self, ApiError> {
        let plan = Plan {
            location: DataLocation { dir: dir.to_path_buf(), source: DirSource::Recorded },
            pointer: dir.join(yanmo_core::paths::LOCATION_FILE),
            suggestion: None,
            sources: Sources::from_env(dir.to_path_buf()),
        };
        Self::open_at(plan, dir.join(EXPORT_DIR))
    }

    fn open_at(plan: Plan, export_dir: PathBuf) -> Result<Self, ApiError> {
        let Plan { location, pointer, suggestion, sources } = plan;
        let dir = location.dir.clone();
        std::fs::create_dir_all(&dir).map_err(|e| {
            ApiError::with("shell.data_dir_create_failed", [("path", dir.display().to_string())])
                .caused_by(e)
        })?;
        // 能不能写**真探一下**：只看权限位会被 ACL、只读介质、UAC 重定向骗过。
        // 探不通就明确报错，绝不静默换地方（换了地方作者会以为稿子丢了）。
        if !yanmo_core::paths::is_writable(&dir) {
            return Err(ApiError::with(
                "shell.data_dir_readonly",
                [("path", dir.display().to_string())],
            ));
        }
        // 同一个数据目录只允许一个研墨：两个进程同时写一个库是真实的损坏来源。
        // 放在打开库**之前**——绝不能先开库、再发现自己本来不该开。
        let instance = crate::single::InstanceLock::acquire(&dir)
            .map_err(|()| ApiError::with("shell.already_running", [("path", dir.display().to_string())]))?;
        let db_path = dir.join(yanmo_core::paths::DB_FILE);
        let mut store = Store::open(&db_path).map_err(|e| {
            ApiError::with("shell.db_open_failed", [("path", db_path.display().to_string())])
                .caused_by(e)
        })?;
        // 登记本次会话，同时拿到"上次退得干不干净"的交代
        let session = store
            .begin_session()
            .map_err(|e| ApiError::new("shell.session_begin_failed").caused_by(e))?;
        Ok(Self {
            store: Mutex::new(Some(store)),
            instance: Mutex::new(Some(instance)),
            db_path,
            export_dir,
            pointer,
            location,
            suggestion,
            sources,
            pending: Mutex::new(None),
            session,
            exit_gate_armed: AtomicBool::new(false),
            exit_watch: ExitWatch::default(),
        })
    }

    /// 数据库文件位置——**只读报告**，界面拿它做展示，拿到也改不了。
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// 数据目录（库文件所在的那一层）。
    pub fn data_dir(&self) -> PathBuf {
        self.db_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    }

    /// 是不是第一次用（界面据此弹首启引导）。
    pub fn is_first_run(&self) -> bool {
        self.location.source.is_first_run()
    }

    /// 首启推荐的位置与理由（不是首启就是 `None`）。
    pub fn suggestion(&self) -> Option<&Suggestion> {
        self.suggestion.as_ref()
    }

    /// 定位置时问到的事实（"选新位置"要拿它算风险）。
    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    /// 位置记录文件（界面只拿它做展示）。
    pub fn pointer(&self) -> &Path {
        &self.pointer
    }

    /// 首启确认「就用这里」：把当前目录记下来，之后不再弹引导。
    pub fn confirm_location(&self) -> Result<(), ApiError> {
        location::write_record(&self.pointer, &self.data_dir()).map_err(ApiError::from)
    }

    /// 记下界面刚选好的新位置（**路径只留在壳里**，界面拿不回去）。
    pub fn set_pending_dir(&self, dir: PathBuf) {
        if let Ok(mut guard) = self.pending.lock() {
            *guard = Some(dir);
        }
    }

    /// 取出待确认的新位置。
    pub fn take_pending_dir(&self) -> Option<PathBuf> {
        self.pending.lock().ok().and_then(|mut guard| guard.take())
    }

    /// 换位置：**复制 → 核对 → 记下新位置**；哪一步不成，原位置与记录都不动。
    ///
    /// 返回成功意味着"新位置已经有一份核对过的稿子"，界面接着重启壳（与换库同一条路）。
    /// **旧位置一字不删**：删不删由作者自己看过之后定——那是唯一一份稿子的备份。
    pub fn relocate(&self, target: &Path) -> Result<Relocation, ApiError> {
        let from = self.data_dir();
        // 先把这次会话**正常收尾**：否则复制过去的那份库里留着一个"没关干净"的记录，
        // 下次在新位置打开时会弹一句"上次异常退出"的假警报（作者刚搬完家，最怕这种吓人话）。
        self.with_store(|store: &mut Store| store.abandon_session())?;
        self.close_store()?;
        let outcome = yanmo_core::store::copy_dir(&from, target)
            .map_err(ApiError::from)
            .and_then(|report| {
                yanmo_core::store::verify_same_scale(target, &from).map_err(ApiError::from)?;
                location::write_record(&self.pointer, target).map_err(ApiError::from)?;
                Ok(report)
            });
        if outcome.is_err() {
            // 没搬成：把原库重新挂上，让作者接着写（不能留在"没有库"的状态）
            self.reopen_store()?;
        }
        outcome
    }

    /// 上一次会话留下的交代（崩溃检测结果）。
    pub fn session(&self) -> &SessionReport {
        &self.session
    }

    /// 界面已就绪：从现在起关窗会先过闸门（先落盘，再决定放不放行）。
    pub fn arm_exit_gate(&self) {
        self.exit_gate_armed.store(true, Ordering::SeqCst);
    }

    /// 闸门是否已武装——未武装时关窗直接放行，免得界面出问题时窗口关不掉。
    pub fn exit_gate_armed(&self) -> bool {
        self.exit_gate_armed.load(Ordering::SeqCst)
    }

    /// 关窗请求的兜底时钟（界面不回话时由它放行退出）。
    pub fn exit_watch(&self) -> &ExitWatch {
        &self.exit_watch
    }

    /// 逃生导出目录（**路径策略在壳，原子写在核心**）。
    pub fn escape_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(ESCAPE_DIR)
    }

    /// 把渲染好的文件写进这本书的导出目录，并清掉上次导出、这次不再需要的残留。
    ///
    /// 两条讲究：
    /// - **内容一样就不重写**：不白改 mtime，也让同步 / 版本工具少扫一遍；
    /// - **清残留**：改了名、删了章之后，上一次导出的旧文件不该留在那儿当孤儿
    ///   （只清理我们自己的 txt / json，且绝不出这个文件夹）。
    pub fn write_export(
        &self,
        work_title: &str,
        files: &[yanmo_core::store::RenderedFile],
    ) -> Result<ExportOutcome, ApiError> {
        let dir = self
            .export_dir
            .join(yanmo_core::atomic::safe_file_name(work_title));
        for file in files {
            let path = dir.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ApiError::with(
                        "shell.export_dir_create_failed",
                        [("path", parent.display().to_string())],
                    )
                    .caused_by(e)
                })?;
            }
            let unchanged = std::fs::read_to_string(&path)
                .map(|old| old == file.content)
                .unwrap_or(false);
            if unchanged {
                continue;
            }
            yanmo_core::atomic::write_atomic(&path, file.content.as_bytes()).map_err(|e| {
                ApiError::with("shell.export_write_failed", [("path", path.display().to_string())])
                    .caused_by(e)
            })?;
        }
        let removed = prune_export(&dir, files)?;
        Ok(ExportOutcome {
            dir,
            files: files.len(),
            removed,
        })
    }

    /// 在数据句柄上跑一段核心操作。
    ///
    /// 壳只负责**串行化（一把锁）与错误转换**，业务一律在核心：
    /// 这样命令层永远不会长出第二套数据逻辑。
    pub fn with_store<T>(
        &self,
        op: impl FnOnce(&mut Store) -> yanmo_core::Result<T>,
    ) -> Result<T, ApiError> {
        let mut guard = self.store.lock().map_err(|_| ApiError::new("shell.store_unavailable"))?;
        let store = guard.as_mut().ok_or_else(|| ApiError::new("shell.store_closed"))?;
        op(store).map_err(ApiError::from)
    }

    /// 关库：把连接收干净。
    ///
    /// 换库之前**必须**先做这一步——文件还开着就动它，等于拿作者的稿子冒险。
    /// 关掉最后一个连接时 SQLite 会把 WAL 归并回主库，这正是留底需要的状态。
    pub fn close_store(&self) -> Result<(), ApiError> {
        let mut guard = self.store.lock().map_err(|_| ApiError::new("shell.store_unavailable"))?;
        guard.take();
        Ok(())
    }

    /// 把库重新挂上（启动时用的是 `open`；"换库失败要回到原状"时走这条）。
    pub fn reopen_store(&self) -> Result<(), ApiError> {
        let mut guard = self.store.lock().map_err(|_| ApiError::new("shell.store_unavailable"))?;
        if guard.is_some() {
            return Ok(());
        }
        let store = Store::open(&self.db_path).map_err(|error| {
            ApiError::with("shell.store_reopen_failed", [("path", self.db_path.display().to_string())])
                .caused_by(error)
        })?;
        *guard = Some(store);
        Ok(())
    }

    /// 换库：**先关库 → 留底 → 换库**；失败时把原库重新挂上再报错。
    ///
    /// 返回成功就意味着数据目录里已经是备份里那一份了；调用方接着重启壳。
    /// **换了库却打不开软件，比不换还糟**——所以这条路上没有"半途而废"这个状态。
    pub fn restore_apply(
        &self,
        source: &Path,
        tz_offset_minutes: i32,
    ) -> Result<yanmo_core::store::RestoreOutcome, ApiError> {
        let data_dir = self
            .db_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| Path::new(".").to_path_buf());
        self.close_store()?;
        let stamp =
            yanmo_core::time::local_stamp(yanmo_core::time::now_millis(), tz_offset_minutes);
        match yanmo_core::store::swap_in(source, &data_dir, &stamp) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                // 换库没成：原库已经被 swap_in 搬回原处，重新挂上让作者接着写
                self.reopen_store()?;
                Err(ApiError::from(error))
            }
        }
    }

    /// 把单实例守卫收出来交给调用方（换库重启用）。
    ///
    /// 重启是**先起新进程、旧进程随后退出**：新进程要在启动期来要同一把锁，
    /// 所以旧进程必须**在换代之前**松开它，否则新进程只会看见「已经在运行」。
    pub fn take_instance_lock(&self) -> Option<crate::single::InstanceLock> {
        self.instance.lock().ok().and_then(|mut guard| guard.take())
    }

    /// 库内实际的数据结构版本（迁移完成后应与引擎期望值一致）。
    pub fn schema_version(&self) -> Result<u32, ApiError> {
        self.with_store(|store| yanmo_core::db::migrations::user_version(store.conn()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个"渲染好的文件"，省得每个测试都写一遍。
    fn file(path: &str, content: &str) -> yanmo_core::store::RenderedFile {
        yanmo_core::store::RenderedFile {
            relative_path: path.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn open_creates_database_where_it_says() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        assert_eq!(data.db_path(), dir.path().join(yanmo_core::paths::DB_FILE).as_path());
        assert!(data.db_path().exists(), "启动后数据库文件应当已落盘");
        assert_eq!(
            data.schema_version().unwrap(),
            yanmo_core::db::migrations::schema_version()
        );
    }

    #[test]
    fn reopening_existing_database_keeps_schema() {
        let dir = tempfile::tempdir().unwrap();
        drop(AppData::open_at_for_test(dir.path()).unwrap());
        let again = AppData::open_at_for_test(dir.path()).unwrap();
        assert_eq!(
            again.schema_version().unwrap(),
            yanmo_core::db::migrations::schema_version()
        );
    }

    #[test]
    fn given_dir_that_cannot_be_created_it_fails_loudly() {
        // 用一个「父级是文件」的路径：建目录必然失败，必须报错而不是静默降级
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let err = match AppData::open_at_for_test(&blocker.join("sub")) {
            Ok(_) => panic!("父级是文件时不应打开成功"),
            Err(e) => e,
        };
        assert_eq!(err.code, "shell.data_dir_create_failed", "错误码应当指明失败原因：{err:?}");
    }

    #[test]
    fn escape_dir_sits_next_to_the_database_and_gate_starts_disarmed() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        assert_eq!(data.escape_dir(), dir.path().join(ESCAPE_DIR));
        assert!(!data.exit_gate_armed(), "界面没就绪前不该拦关窗");
        data.arm_exit_gate();
        assert!(data.exit_gate_armed());
    }

    #[test]
    fn export_writes_files_and_leaves_the_second_run_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let files = vec![
            file("001-第一卷/001-第一章.txt", "第一章的正文。\n"),
            file("001-第一卷/002-第二章.txt", "第二章的正文。\n"),
        ];

        let first = data.write_export("长夜", &files).unwrap();
        assert_eq!(first.files, 2);
        assert_eq!(first.removed, 0);
        assert!(first.dir.starts_with(dir.path()), "导出只该落在自己的导出目录里");
        assert_eq!(
            std::fs::read_to_string(first.dir.join("001-第一卷/001-第一章.txt")).unwrap(),
            "第一章的正文。\n"
        );

        // 再导一次：内容没变，文件不该被动过（mtime 也不该变）
        let stamp = std::fs::metadata(first.dir.join("001-第一卷/001-第一章.txt"))
            .unwrap()
            .modified()
            .unwrap();
        let second = data.write_export("长夜", &files).unwrap();
        assert_eq!(second.files, 2);
        assert_eq!(second.removed, 0);
        assert_eq!(
            std::fs::metadata(second.dir.join("001-第一卷/001-第一章.txt"))
                .unwrap()
                .modified()
                .unwrap(),
            stamp,
            "内容一样就不该重写"
        );
    }

    #[test]
    fn export_prunes_stale_files_but_never_touches_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let before = vec![
            file("001-第一卷/001-旧章.txt", "旧正文\n"),
            file("001-第一卷/002-留着的.txt", "留下的正文\n"),
        ];
        data.write_export("长夜", &before).unwrap();

        // 作者自己在导出夹里放了个东西：**不碰**
        let mine = data.export_dir.join("长夜").join("我的笔记.md");
        std::fs::write(&mine, "手工写的").unwrap();

        // 旧章改名了：上一次的文件应当被清掉，空目录也收掉
        let after = vec![file("001-第一卷/002-留着的.txt", "留下的正文\n")];
        let outcome = data.write_export("长夜", &after).unwrap();
        assert_eq!(outcome.removed, 1);
        assert!(!outcome.dir.join("001-第一卷/001-旧章.txt").exists(), "旧的该清掉");
        assert!(outcome.dir.join("001-第一卷/002-留着的.txt").exists());
        assert!(mine.exists(), "作者自己放的文件一个都不许碰");
    }

    #[test]
    fn export_folder_cannot_escape_the_export_root() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let outcome = data
            .write_export("../../跑出去的书名", &[file("a.txt", "x\n")])
            .unwrap();
        assert!(
            outcome.dir.starts_with(&data.export_dir),
            "书名里的路径分隔符要被安全化，导出不许跑到目录外面去：{}",
            outcome.dir.display()
        );
    }

    #[test]
    fn a_second_launch_reports_the_previous_one_as_unclean() {
        let dir = tempfile::tempdir().unwrap();
        let first = AppData::open_at_for_test(dir.path()).unwrap();
        assert!(!first.session().unclean, "第一次启动不该报崩溃");
        drop(first); // 没走正常退出 = 被杀
        let second = AppData::open_at_for_test(dir.path()).unwrap();
        assert!(second.session().unclean, "第二次启动必须报出上次是异常退出");
    }

    // ── 单实例守卫：同一个数据目录不许开第二个 ────────────────────────────────

    #[test]
    fn a_second_instance_on_the_same_data_directory_is_refused_and_can_start_after_release() {
        let dir = tempfile::tempdir().unwrap();
        let first = AppData::open_at_for_test(dir.path()).unwrap();

        let error = match AppData::open_at_for_test(dir.path()) {
            Ok(_) => panic!("同一个数据目录不该能开第二个"),
            Err(error) => error,
        };
        assert_eq!(error.code, "shell.already_running", "{error:?}");
        assert_eq!(
            error.params.get("path").map(String::as_str),
            Some(dir.path().to_string_lossy().as_ref()),
            "提示里要说清是哪个数据目录"
        );

        drop(first);
        assert!(
            AppData::open_at_for_test(dir.path()).is_ok(),
            "关掉之后必须还能再开（否则软件就永远打不开了）"
        );
    }

    // ── 换库这条路的**壳侧**验收：关库 → 换库 → （失败）重挂 ────────────────────
    // 核心那边验的是文件动作；这里验的是壳有没有把"库句柄"这件事管对：
    // 换库必须先把连接收干净，失败之后软件还得能用。全程只碰临时目录。

    /// 造一本书 + 一章，并把它备份到一个"备份盘"目录。
    ///
    /// 返回 `(那份包的路径, 这一章的 node_id)`——节点 id 不能猜（作品会自带一个默认卷）。
    fn seed_and_backup(data: &AppData, dir: &Path, target: &Path) -> (std::path::PathBuf, i64) {
        let chapter = data
            .with_store(|store| {
                let work = store.create_work(yanmo_core::model::WorkKind::Novel, "长夜")?;
                let volume = store.list_nodes(work.id)?[0].id;
                let chapter = store.create_node(
                    work.id,
                    Some(volume),
                    yanmo_core::model::NodeKind::Chapter,
                    "第一章",
                )?;
                store.write_body(chapter, "雨下了整夜。")?;
                Ok(chapter)
            })
            .unwrap();
        assert!(chapter > 0);

        let request = yanmo_core::store::BackupRequest {
            data_dir: dir.to_path_buf(),
            targets: vec![yanmo_core::store::BackupTarget {
                path: target.to_string_lossy().to_string(),
                volume_id: "vol-other".to_string(),
                volume_label: "备份盘".to_string(),
                removable: true,
            }],
            keep: 7,
            tz_offset_minutes: 480,
            device: "测试机".to_string(),
        };
        let report = data.with_store(|store| Ok(store.backup_now(&request)?)).unwrap();
        assert_eq!(report.succeeded(), 1, "备份该成功：{:?}", report.outcomes);
        (std::path::PathBuf::from(&report.outcomes[0].package), chapter)
    }

    #[test]
    fn a_restore_that_fails_puts_the_database_back_and_the_app_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("备份盘");
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let (_package, chapter) = seed_and_backup(&data, dir.path(), &target);

        // 一份打不开的"库"：换库会在体检那一步失败
        let junk = dir.path().join("手选来的.db");
        std::fs::write(&junk, "这不是一个 SQLite 库").unwrap();
        let error = data.restore_apply(&junk, 480).unwrap_err();
        assert_eq!(error.code, "backup.restore_swap_failed", "{error:?}");

        // ★ 失败之后**库要重新挂上**：换了库却打不开软件，比不换还糟
        let alive = data.with_store(|store| store.read_body(chapter)).unwrap();
        assert_eq!(alive, "雨下了整夜。", "原库重新挂上，作者能接着写");
    }

    #[test]
    fn a_restore_that_succeeds_leaves_a_closed_store_and_a_kept_old_database() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("备份盘");
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let (package, chapter) = seed_and_backup(&data, dir.path(), &target);

        // 备份之后改掉正文：恢复会把这一版"退回去"
        data.with_store(|store| Ok(store.write_body(chapter, "备份之后写的这一版。")?)).unwrap();
        let outcome = data.restore_apply(&package, 480).unwrap();

        // 换库成功时库句柄是**关着**的（壳马上要重启）——这里对应用户看到"正在重新打开"
        let closed = data.with_store(|store| store.read_body(chapter)).unwrap_err();
        assert_eq!(closed.code, "shell.store_closed");

        // 重启那一下就是"重挂"：挂上之后读到的是备份里那一版
        data.reopen_store().unwrap();
        let body = data.with_store(|store| store.read_body(chapter)).unwrap();
        assert_eq!(body, "雨下了整夜。");

        // 原库留底留得住（含"备份之后写的那一版"）
        let kept = Path::new(&outcome.quarantine).join(yanmo_core::paths::DB_FILE);
        assert!(kept.is_file(), "原库要留底：{}", kept.display());
    }

    // ── 换位置这条路的**壳侧**验收：复制 → 核对 → 记下新位置 ──────────────────
    // 核心那边验的是"复制与核对"；这里验的是壳有没有把三件事做对：
    // ① 搬成功之后库句柄是关着的（马上要重启）；② 旧位置一个字不删；③ 没搬成就还能接着写。

    /// 造一本一章的书，返回那一章的 id。
    fn seed_chapter(data: &AppData) -> i64 {
        data.with_store(|store| {
            let work = store.create_work(yanmo_core::model::WorkKind::Novel, "长夜")?;
            let volume = store.list_nodes(work.id)?[0].id;
            let chapter = store.create_node(
                work.id,
                Some(volume),
                yanmo_core::model::NodeKind::Chapter,
                "第一章",
            )?;
            store.write_body(chapter, "雨下了整夜。")?;
            Ok(chapter)
        })
        .unwrap()
    }

    #[test]
    fn moving_the_library_copies_it_records_the_new_place_and_keeps_the_old_one() {
        let home = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(home.path()).unwrap();
        let chapter = seed_chapter(&data);
        let target = elsewhere.path().join("我的稿子");

        let report = data.relocate(&target).unwrap();
        assert_eq!(report.dir, target);
        assert!(report.files >= 1, "至少库文件要复制过去：{report:?}");

        // ① 搬完库句柄是关着的：壳马上要重启（界面此时显示"正在重新打开"）
        let closed = data.with_store(|store| store.read_body(chapter)).unwrap_err();
        assert_eq!(closed.code, "shell.store_closed");

        // ② 旧位置**一个字都不删**（删不删是作者看过新位置之后的事）
        assert!(
            home.path().join(yanmo_core::paths::DB_FILE).is_file(),
            "旧位置必须原样留着"
        );

        // ③ 位置记录写的是新位置
        let pointer = home.path().join(yanmo_core::paths::LOCATION_FILE);
        assert_eq!(
            yanmo_core::location::read_record(&pointer),
            Some(target.clone()),
            "记录没写对，下次启动就会回到旧位置"
        );

        // ④ 新位置那份库真能用（直接打开读回来）
        let moved = yanmo_core::store::Store::open(target.join(yanmo_core::paths::DB_FILE)).unwrap();
        assert_eq!(moved.read_body(chapter).unwrap(), "雨下了整夜。");
    }

    #[test]
    fn a_move_that_cannot_copy_leaves_the_old_place_and_the_record_alone() {
        let home = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(home.path()).unwrap();
        let chapter = seed_chapter(&data);

        // 目标里已经有一份稿子：壳必须**拒绝**，绝不覆盖别人的库
        let occupied = elsewhere.path().join("已经有一份");
        std::fs::create_dir_all(&occupied).unwrap();
        yanmo_core::store::Store::open(occupied.join(yanmo_core::paths::DB_FILE)).unwrap();

        let error = data.relocate(&occupied).unwrap_err();
        assert_eq!(error.code, "store.relocate_target_in_use", "{error:?}");

        // ★ 没搬成：库要重新挂上（作者还得接着写），记录也不能动
        let alive = data.with_store(|store| store.read_body(chapter)).unwrap();
        assert_eq!(alive, "雨下了整夜。");
        assert_eq!(
            yanmo_core::location::read_record(&home.path().join(yanmo_core::paths::LOCATION_FILE)),
            None,
            "没搬成就不该留下记录"
        );
    }

    #[test]
    fn a_move_never_lands_inside_the_current_folder() {
        let home = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(home.path()).unwrap();
        let chapter = seed_chapter(&data);

        // 往自己里面搬：会把稿子复制进自己的子目录（而且越复制越多）
        let inside = home.path().join("子目录/我的稿子");
        let error = data.relocate(&inside).unwrap_err();
        assert_eq!(error.code, "store.relocate_inside", "{error:?}");
        assert_eq!(data.with_store(|store| store.read_body(chapter)).unwrap(), "雨下了整夜。");
    }

    #[test]
    fn confirming_the_first_run_writes_the_record_and_cancelling_drops_the_pick() {
        let home = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(home.path()).unwrap();

        data.confirm_location().unwrap();
        let pointer = home.path().join(yanmo_core::paths::LOCATION_FILE);
        assert_eq!(
            yanmo_core::location::read_record(&pointer),
            Some(home.path().to_path_buf()),
            "确认之后必须记得住"
        );

        // 选中 → 取走（壳只让取一次：取走了界面就没法拿旧选择再搬一遍）
        data.set_pending_dir(home.path().join("新家"));
        assert_eq!(data.take_pending_dir(), Some(home.path().join("新家")));
        assert_eq!(data.take_pending_dir(), None);
    }
}
