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
use crate::export_fs::{
    export_folder_name, prune_export, read_export_manifest, write_export_manifest, write_files,
};

/// 一次导出的结果（类型与落盘口径都在 [`crate::export_fs`]，这里转出去给命令层用）。
pub use crate::export_fs::ExportOutcome;

/// 系统答不上"文档在哪"时的导出落点：数据目录里的一个子目录。
const EXPORT_DIR: &str = "export";
// 库文件名与"文档/导出目录"的名字**不写在本文件**：图形界面与命令行救援入口是两个壳，
// 它们必须认同同一份约定（常量与选择规则都在 `yanmo_core::paths`）——
// 各写一份的结果是改一处漏一处，作者会以为稿子分家了。

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
    /// 磁盘 `.md` 镜像的工作线程（`setup` 里挂上；验收模式与命令行没有它 → `None`）。
    ///
    /// 它落在壳里而不是核心：核心零 UI 依赖、也不该自己起线程；镜像要写文件、要异步，
    /// 这两件事本来就归壳（同 [`AppData::write_export`]）。
    mirror: Mutex<Option<crate::mirror::MirrorHandle>>,
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
        //
        // ⚠️ **便携模式下不为"程序旁的 data/"写记录**：数据目录由规则确定，没什么要记的；
        // 而记录存的是绝对路径——写下来之后文件夹改名会凭空建出一个空库、U 盘换盘符会
        // 直接报"数据目录不可用"，"拔盘即走"当场变味（见 `location::is_portable_default`）。
        if !data.location.source.is_first_run()
            && data.location.source != DirSource::Recorded
            && !location::is_portable_default(data.sources(), &data.data_dir())
        {
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
        // **位置记录指向的目录里没有库**：绝不在原地开一个新库——那会让作者看到一个空书架，
        // 以为稿子没了（2026-09-15 代码质量评审：严重 3；location.rs 的注释里早就点过这个场景，
        // 只是没堵住"作者亲手选过目录"这一半）。明确拒绝，把路径与位置记录文件都交给界面说清楚。
        if location.source == DirSource::RecordedMissing {
            return Err(ApiError::with(
                "shell.recorded_dir_missing_db",
                [
                    ("path", dir.display().to_string()),
                    ("pointer", pointer.display().to_string()),
                ],
            ));
        }
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
            mirror: Mutex::new(None),
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
    ///
    /// 便携模式下如果是"程序旁的 data/"，**不用记**（规则已经能算出它在哪）——
    /// 记成绝对路径反而会让改名/换盘符变成"打开一个空库"。
    pub fn confirm_location(&self) -> Result<(), ApiError> {
        if location::is_portable_default(self.sources(), &self.data_dir()) {
            return Ok(());
        }
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
        // **先把镜像线程收干净**：它可能正握着一份 `.md` 在写，而紧接着整份数据目录要被复制走
        // ——复制到一半的那些文件在新位置上会被当成"外面的改动"，白白报一堆假冲突。
        self.stop_mirror();
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
            // 没搬成：把原库重新挂上，让作者接着写（不能留在"没有库"的状态）；
            // 镜像线程也一并起回来——停在原地却不再跟稿子，比不做镜像更糟（作者会以为它还在）。
            self.reopen_store()?;
            self.restart_mirror();
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

    /// 镜像根目录：数据目录里的 `mirror/`。
    ///
    /// 为什么放在稿库里面（而不是"文档"下另起一处）：它要**跟着稿子走**——换位置、便携模式
    /// 拔盘带走、换库，都是整份数据目录的事；另起一处迟早会出现"镜像还是上一本书的"。
    /// 想找它的作者有「打开镜像文件夹」那个入口，不必自己记路径。
    pub fn mirror_root(&self) -> PathBuf {
        self.data_dir().join(crate::mirror::MIRROR_DIR)
    }

    /// 挂上镜像工作线程（`setup` 里 manage 之后调一次）。
    pub fn attach_mirror(&self, mirror: crate::mirror::MirrorHandle) {
        if let Ok(mut slot) = self.mirror.lock() {
            *slot = Some(mirror);
        }
    }

    /// 镜像句柄（验收模式与命令行没有 → `None`）。
    pub fn mirror(&self) -> Option<crate::mirror::MirrorHandle> {
        self.mirror.lock().ok().and_then(|slot| slot.clone())
    }

    /// 告诉镜像"刚落了盘"——**不阻塞、不排队**（它在后台自己对账）。
    pub fn poke_mirror(&self) {
        if let Some(mirror) = self.mirror() {
            mirror.poke();
        }
    }

    /// 让镜像先做一次全量核对（作者点「立即同步」）。
    pub fn force_mirror(&self) {
        if let Some(mirror) = self.mirror() {
            mirror.force_sync();
        }
    }

    /// 停机并等镜像线程真的停下（搬迁前必须做，见 [`crate::mirror::MirrorHandle::stop_and_join`]）。
    pub fn stop_mirror(&self) {
        if let Some(mirror) = self.mirror() {
            mirror.stop_and_join();
        }
    }

    /// 把镜像线程重新起起来（搬迁没成、留在原处时用）。
    pub fn restart_mirror(&self) {
        if let Some(old) = self.mirror() {
            if let Ok(mut slot) = self.mirror.lock() {
                *slot = Some(old.restart());
            }
        }
    }

    /// 逃生导出的候选落点，按"最不容易与故障同源"排序（顺序与理由见 [`crate::escape`]）。
    pub fn escape_candidates(&self) -> Vec<PathBuf> {
        crate::escape::candidates(&self.db_path)
    }

    /// 把渲染好的文件写进这本书的导出目录，并清掉上次导出、这次不再需要的残留。
    ///
    /// 两条讲究：
    /// - **内容一样就不重写**：不白改 mtime，也让同步 / 版本工具少扫一遍；
    /// - **清残留**：改了名、删了章之后，上一次导出的旧文件不该留在那儿当孤儿
    ///   （只清理我们自己的 txt / json，且绝不出这个文件夹）。
    pub fn write_export(
        &self,
        work_id: i64,
        work_title: &str,
        files: &[yanmo_core::store::RenderedFile],
    ) -> Result<ExportOutcome, ApiError> {
        let dir = self.export_dir.join(export_folder_name(work_id, work_title));
        let previous = read_export_manifest(&dir);
        write_files(&dir, files)?;
        let removed = prune_export(&dir, &previous, files, &["txt", "json"])?;
        write_export_manifest(&dir, files);
        Ok(ExportOutcome {
            dir,
            files: files.len(),
            removed,
        })
    }

    /// 某种预设的产物目录：`<导出目录>/<书名>/<预设子目录>`。
    ///
    /// 路径**在壳里算**，界面只说要哪本书、哪种预设——与"命令不接受路径参数"同一条纪律。
    /// 写产物与"打开这个目录"都用它，两处不会走偏。
    pub fn compile_dir(&self, work_id: i64, work_title: &str, folder: &str) -> std::path::PathBuf {
        self.export_dir.join(export_folder_name(work_id, work_title)).join(folder)
    }

    /// 把**编译产物**写进 `<导出目录>/<书名>/<预设子目录>`，并只清这个子目录里的旧产物。
    ///
    /// 为什么按子目录清：换一种预设编译时，不能把上一种的产物（作者刚拿去投稿的那份）删掉。
    /// 产物路径里带着子目录前缀（`submission/长夜.docx`），这里把前缀摘掉再落到子目录里。
    pub fn write_compile(
        &self,
        work_id: i64,
        work_title: &str,
        folder: &str,
        extension: &str,
        files: &[yanmo_core::store::RenderedFile],
    ) -> Result<ExportOutcome, ApiError> {
        let dir = self.compile_dir(work_id, work_title, folder);
        let prefix = format!("{folder}/");
        let stripped: Vec<yanmo_core::store::RenderedFile> = files
            .iter()
            .map(|file| yanmo_core::store::RenderedFile {
                relative_path: file
                    .relative_path
                    .strip_prefix(&prefix)
                    .unwrap_or(&file.relative_path)
                    .to_string(),
                content: file.content.clone(),
            })
            .collect();
        let previous = read_export_manifest(&dir);
        write_files(&dir, &stripped)?;
        let removed = prune_export(&dir, &previous, &stripped, &[extension])?;
        write_export_manifest(&dir, &stripped);
        Ok(ExportOutcome {
            dir,
            files: stripped.len(),
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
        // **库文件不在原位就别挂**：`Store::open` 会凭空建一个新库，而这条路的调用者
        // 是"换库失败要回到原状"——真库没搬回来时在原地建个空库，界面就会若无其事地
        // 跑在一本空书架上（2026-09-15 代码质量评审：中等 3）。宁可明确报"库不在"。
        if !self.db_path.is_file() {
            return Err(ApiError::with(
                "shell.store_reopen_missing",
                [("path", self.db_path.display().to_string())],
            ));
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
                // 换库没成：原库本该已经被 swap_in 搬回原处，重新挂上让作者接着写。
                //
                // 但如果**它没搬回来**（回滚也失败）——原位没有库文件——就绝不能再挂一个空库顶上：
                // 那会让界面看起来一切正常、其实跑在空书架上（2026-09-15 代码质量评审：中等 3）。
                // 这时把两件事一起报出去：换库为什么失败 + 库不在原位（真库多半在「旧库留底」里）。
                if let Err(reopen) = self.reopen_store() {
                    return Err(ApiError::with(
                        "shell.restore_left_without_library",
                        [("path", self.db_path.display().to_string())],
                    )
                    .caused_by(format!("{error}／{reopen}")));
                }
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
        yanmo_core::store::RenderedFile::text(path, content.to_string())
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

    /// **库文件不在原位时，重新挂库必须拒绝**（2026-09-15 代码质量评审：中等 3）。
    ///
    /// 这条路的调用者是"换库失败要回到原状"：如果真库没搬回来（回滚也失败），
    /// `Store::open` 会在原地建出一个空库——界面于是若无其事地跑在一本空书架上。
    /// 宁可明确报"库不在"，也不许凭空造一本。
    #[test]
    fn reopening_refuses_when_the_library_is_not_there() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let db = data.db_path().to_path_buf();
        assert!(db.is_file(), "前提：启动后库文件在");

        data.close_store().unwrap();
        std::fs::remove_file(&db).unwrap(); // 模拟"原库没能回到原位"

        let error = data.reopen_store().expect_err("库不在原位时不许挂，更不许建一个新的");
        assert_eq!(error.code, "shell.store_reopen_missing");
        assert!(!db.exists(), "拒绝之后不许留下一个空库");
    }

    #[test]
    fn the_escape_directory_hangs_off_the_data_dir_and_the_gate_starts_disarmed() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        assert_eq!(data.escape_candidates().last(), Some(&crate::escape::dir(data.db_path())));
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

        let first = data.write_export(1, "长夜", &files).unwrap();
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
        let second = data.write_export(1, "长夜", &files).unwrap();
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
        data.write_export(1, "长夜", &before).unwrap();

        // 作者自己在导出夹里放了个东西：**不碰**
        let mine = data.export_dir.join("长夜-1").join("我的笔记.md");
        std::fs::write(&mine, "手工写的").unwrap();

        // 旧章改名了：上一次的文件应当被清掉，空目录也收掉
        let after = vec![file("001-第一卷/002-留着的.txt", "留下的正文\n")];
        let outcome = data.write_export(1, "长夜", &after).unwrap();
        assert_eq!(outcome.removed, 1);
        assert!(!outcome.dir.join("001-第一卷/001-旧章.txt").exists(), "旧的该清掉");
        assert!(outcome.dir.join("001-第一卷/002-留着的.txt").exists());
        assert!(mine.exists(), "作者自己放的文件一个都不许碰");
    }

    /// 编译产物落进**自己的预设子目录**；换一种预设不许把上一种的产物删掉。
    ///
    /// 这一条盯的是真踩过的坑：导出那条清理是按整本书目录扫的，编译要是也这么干，
    /// 作者刚拿去投稿的那份 docx 会被下一次"合并 txt"顺手删掉。
    #[test]
    fn compile_keeps_each_preset_in_its_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let docx = vec![yanmo_core::store::RenderedFile {
            relative_path: "submission/长夜.docx".to_string(),
            content: vec![0x50, 0x4b, 0x03, 0x04, 0x00],
        }];

        let first = data.write_compile(1, "长夜", "submission", "docx", &docx).unwrap();
        assert_eq!(first.files, 1);
        assert!(first.dir.ends_with("submission"), "{:?}", first.dir);
        assert_eq!(
            std::fs::read(first.dir.join("长夜.docx")).unwrap(),
            vec![0x50, 0x4b, 0x03, 0x04, 0x00],
            "二进制产物要原样落盘"
        );

        // 换一种预设：它的产物不许被动
        let merged = vec![file("merged/长夜.txt", "全文\n")];
        data.write_compile(1, "长夜", "merged", "txt", &merged).unwrap();
        assert!(first.dir.join("长夜.docx").exists(), "换预设不该动上一种的产物");

        // 同一种预设再编译一次，上一次多出来的那个要清掉
        let stale = vec![
            yanmo_core::store::RenderedFile {
                relative_path: "submission/长夜.docx".to_string(),
                content: vec![1],
            },
            yanmo_core::store::RenderedFile {
                relative_path: "submission/旧稿.docx".to_string(),
                content: vec![2],
            },
        ];
        data.write_compile(1, "长夜", "submission", "docx", &stale).unwrap();
        let outcome = data.write_compile(1, "长夜", "submission", "docx", &docx).unwrap();
        assert_eq!(outcome.removed, 1, "上一次多出来的产物要清掉");
        assert!(!first.dir.join("旧稿.docx").exists());
        assert!(first.dir.join("长夜.docx").exists());
    }

    #[test]
    fn export_folder_cannot_escape_the_export_root() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let outcome = data
            .write_export(1, "../../跑出去的书名", &[file("a.txt", "x\n")])
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

    /// **导出一本书不许删掉另一本书的产物**（2026-09-15 代码质量评审：中等 17）。
    ///
    /// 目录名按书名归一化，而重名作品是允许的：以前"清残留"会把目录里所有不在本次清单里的
    /// txt/json 删掉——于是 B 的导出顺手删掉 A 刚导出的那一份，还会删掉同一本书上次的编译产物。
    /// 现在只按**自己的清单**清。
    #[test]
    fn exporting_does_not_delete_another_books_files() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let book = dir.path().join(EXPORT_DIR).join("长夜-1");

        data.write_export(
            1,
            "长夜",
            &[file("第一章.txt", "A1"), file("第二章.txt", "A2"), file("第三章.txt", "A3")],
        )
        .unwrap();
        // 另一本同名的书（两本重名是允许的）：它只写两个文件
        data.write_export(2, "长夜", &[file("第一章.txt", "B1"), file("第二章.txt", "B2")])
            .unwrap();

        assert!(book.join("第三章.txt").is_file(), "B 的导出不许删掉 A 的文件");
    }

    /// 反向钉子：**同一本书再导出，上次多出来的那个还得清掉**（这是"清残留"的本职）。
    #[test]
    fn exporting_the_same_book_again_still_clears_its_own_stale_files() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_at_for_test(dir.path()).unwrap();
        let book = dir.path().join(EXPORT_DIR).join("长夜-1");

        data.write_export(1, "长夜", &[file("第一章.txt", "v1"), file("第二章.txt", "v1")]).unwrap();
        data.write_export(1, "长夜", &[file("第一章.txt", "v2")]).unwrap();

        assert_eq!(std::fs::read_to_string(book.join("第一章.txt")).unwrap(), "v2");
        assert!(!book.join("第二章.txt").exists(), "上次多出来那个要清掉");
    }
}
