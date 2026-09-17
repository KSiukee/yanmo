//! 磁盘 `.md` 强镜像的**落盘侧**：一个后台线程 + 一份对账结果。
//!
//! # 分工
//!
//! 核心管"该有哪些文件、怎么对"（`yanmo_core::store::mirror_plan` 是决策、
//! `yanmo_core::store::mirror` 是账本与渲染，两处都可单测）；这里只做三件核心不做的事：
//!
//! 1. **异步**：落盘之后投一个信号就走，绝不挡击键（与"边写边存"同一条精神）；
//! 2. **落盘**：真的读写文件——探磁盘、原子写、`rename`、收残留，全在
//!    [`crate::mirror_fs`]（那一层不碰数据库、也不碰线程，单独可测）；
//! 3. 把"上次对完账的样子"记在内存里，供界面如实报告（有几份被人改过、上次对上是什么时候）。
//!
//! # 为什么与导出不共用一条路
//!
//! 导出是"作者点一下、把成稿带走"（一次性、有清单清残留）；镜像是"一直跟着稿子走"
//! （增量、改名要 `rename`、**外面的改动一个字不碰**）。两者的取舍不同，共用一个函数
//! 迟早为了一边牺牲另一边。真正共用的在更下面：路径口径（`tree_path`）与原子写（`atomic`）。
//!
//! # 节拍
//!
//! 落盘命令投一次信号（合并成一次，见 [`MirrorHandle::poke`]），另外**每 2 秒自己巡一遍**
//! （兜底：结构变更、清空回收站这类动作没投信号也照样跟上）；一直没人动就放慢到 15 秒。
//! 巡检分两档：库变了就按账补写；库没变、人也没动时**全量核对一次**（发现文件被删 / 被改）。
//!
//! # 锁的纪律
//!
//! **锁内取数与记账、锁外读写文件**：文件 I/O 不许占着数据锁，那是击键要走的路。
//!
//! # 诚实边界
//!
//! 镜像**不是备份**：它就在稿库同一个盘、同一个目录里，盘坏了它一起没。它的承诺是
//! "稿子永远是你能用记事本打开的 `.md`"（不被格式困住），不是"盘坏了还在"——那是备份的事。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use yanmo_core::store::MirrorEntry;

use crate::error::ApiError;
use crate::mirror_fs::{disk_looks_untouched, execute, probe};
use crate::storage::AppData;

/// 镜像根目录名（**语言无关**：它跟着稿子走，不该随界面语言变）。
pub const MIRROR_DIR: &str = "mirror";

/// 有人动过时的巡检节拍。
const SWEEP: Duration = Duration::from_secs(2);
/// 长时间没人动之后的节拍（省电、也省磁盘）。
const SWEEP_IDLE: Duration = Duration::from_secs(15);
/// 连着这么多次巡检都没活干，就放慢到 [`SWEEP_IDLE`]。
const IDLE_AFTER: u32 = 15;

/// 镜像现在什么样——**只读报告**，界面拿它说话（不改任何东西）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct MirrorStatus {
    /// 镜像开着没有（默认开）
    pub enabled: bool,
    /// 镜像根目录（给作者看；界面拿它只做展示）
    pub root: String,
    /// 上次对完账的时间（毫秒；0 = 还没对过）
    pub last_sync_at: i64,
    /// 账上照看的文件数
    pub files: usize,
    /// **有人在我们之外改过、暂未覆盖**的文件数（下一步给作者定夺：合并 / 覆盖）
    pub conflicts: usize,
    /// 这一轮没对上的书（下一轮会再来）
    pub failed_works: usize,
    /// 最近一次失败的技术说明（空 = 没失败；界面只把它放进日志味的提示里）
    pub last_error: String,
}

struct Inner {
    /// 唤醒信号：容量 1 的通道，**投不满就丢**——连打一百下合成一次对账。
    wake: SyncSender<()>,
    status: Mutex<MirrorStatus>,
    force: AtomicBool,
    stop: AtomicBool,
    join: Mutex<Option<JoinHandle<()>>>,
}

/// 壳持有的镜像句柄（可克隆：命令层、存储层、搬迁那一步都要用它说话）。
#[derive(Clone)]
pub struct MirrorHandle {
    app: AppHandle,
    inner: Arc<Inner>,
}

impl MirrorHandle {
    /// 起线程。**第一趟是全量核对**（把库与磁盘上的账对平，顺手补上历史遗留）。
    pub fn start(app: AppHandle) -> Self {
        let (wake, rx) = mpsc::sync_channel(1);
        let inner = Arc::new(Inner {
            wake,
            status: Mutex::new(MirrorStatus::default()),
            force: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            join: Mutex::new(None),
        });
        let worker = Arc::clone(&inner);
        let thread_app = app.clone();
        let join = std::thread::Builder::new()
            .name("yanmo-mirror".to_string())
            .spawn(move || run(thread_app, rx, worker))
            .ok();
        if let Ok(mut slot) = inner.join.lock() {
            *slot = join;
        }
        Self { app, inner }
    }

    /// 投一次信号（**不阻塞、不排队**：对账合并成一次就够）。
    pub fn poke(&self) {
        let _ = self.inner.wake.try_send(());
    }

    /// 请它做一次全量核对（作者点「立即对一遍」；也用于刚打开镜像时的补齐）。
    pub fn force_sync(&self) {
        self.inner.force.store(true, Ordering::SeqCst);
        self.poke();
    }

    /// 上次对完账的样子。
    pub fn status(&self) -> MirrorStatus {
        self.inner.status.lock().map(|status| status.clone()).unwrap_or_default()
    }

    /// 停机并**等它真的停下**。
    ///
    /// 换位置（搬迁）之前必须做这一步：它可能正握着一份文件在写，而那一刻的目录正要被
    /// 整份复制走——复制到一半的文件在新位置上会被当成"外面的改动"，白白报一堆假冲突。
    pub fn stop_and_join(&self) {
        self.inner.stop.store(true, Ordering::SeqCst);
        self.poke();
        let handle = self.inner.join.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    /// 起一个新的（停机之后要接着用，比如搬迁失败又留在原处）。
    pub fn restart(&self) -> Self {
        Self::start(self.app.clone())
    }
}

/// 线程主循环：等信号 / 到点巡检 → 对账。
fn run(app: AppHandle, rx: Receiver<()>, inner: Arc<Inner>) {
    let mut idle_streak = 0u32;
    let mut booted = false;
    loop {
        let wait = if idle_streak >= IDLE_AFTER { SWEEP_IDLE } else { SWEEP };
        let idle = match rx.recv_timeout(wait) {
            Ok(()) => {
                // 攒着的信号一次抽干：连打期间只对一次账
                while rx.try_recv().is_ok() {}
                false
            }
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if inner.stop.load(Ordering::SeqCst) {
            return;
        }
        let full = !booted || inner.force.load(Ordering::SeqCst);
        booted = true;
        let did_work = sweep(&app, &inner, full, idle);
        if did_work {
            inner.force.store(false, Ordering::SeqCst);
        }
        idle_streak = if did_work { 0 } else { idle_streak.saturating_add(1) };
    }
}

/// 巡一遍：**锁内取数与记账、锁外读写文件**（文件 I/O 不许占着数据锁）。
fn sweep(app: &AppHandle, inner: &Inner, full: bool, idle: bool) -> bool {
    let Some(data) = app.try_state::<AppData>() else {
        return false;
    };
    let root = data.mirror_root();
    let mut report = MirrorStatus {
        root: root.display().to_string(),
        ..MirrorStatus::default()
    };

    match data.with_store(|store| store.mirror_enabled()) {
        Ok(enabled) => report.enabled = enabled,
        Err(error) => {
            report.last_error = technical(&error);
            publish(inner, report);
            return false;
        }
    }
    if !report.enabled {
        report.last_sync_at = yanmo_core::time::now_millis();
        publish(inner, report);
        return false;
    }

    let works = match data.with_store(|store| store.mirror_works()) {
        Ok(works) => works,
        Err(error) => {
            report.last_error = technical(&error);
            publish(inner, report);
            return false;
        }
    };

    let mut did_work = false;
    for work in works {
        // ① 锁内：算出"该有什么"和"账上是什么"（不碰文件）
        let prepared = data.with_store(|store| {
            Ok((
                store.render_mirror(work)?,
                store.mirror_state(work)?,
                store.mirror_pending(work)?,
            ))
        });
        let (desired, state, pending) = match prepared {
            Ok(parts) => parts,
            Err(error) => {
                // 单本书出问题（比如树深过上限）不该让别的书跟着停摆
                report.failed_works += 1;
                report.last_error = technical(&error);
                continue;
            }
        };

        let mut verify = full;
        if !pending {
            // 库没变：日常投来的信号到这儿就结束；只有空闲巡检 / 全量核对才去看磁盘
            if !(idle || full) {
                report.files += state.len();
                continue;
            }
            if disk_looks_untouched(&root, &state) {
                report.files += state.len();
                continue;
            }
            verify = true; // 磁盘上少了、或大小不对 → 这一本全量核一遍
        }

        // ② 锁外：探磁盘 → 算计划 → 落盘
        let account = account_of(&state);
        let plan = yanmo_core::store::plan_mirror(
            &desired,
            &state,
            |path| probe(&root, path, &account),
            verify,
        );
        if let Err(error) = execute(&root, &desired, &plan) {
            // **没落成就不记账**：账留在原样，下一轮从原样再来（写是幂等的，重来不亏）
            report.failed_works += 1;
            report.conflicts += plan.conflicts.len();
            report.last_error = technical(&error);
            continue;
        }

        // ③ 锁内：记账
        if let Err(error) = data.with_store(|store| store.mirror_record(work, &plan.records)) {
            report.failed_works += 1;
            report.last_error = technical(&error);
            continue;
        }
        report.files += plan.records.len();
        report.conflicts += plan.conflicts.len();
        did_work = true;
    }

    report.last_sync_at = yanmo_core::time::now_millis();
    publish(inner, report);
    did_work
}

/// 账按路径索引（探磁盘时按路径查"这一份记的是什么"）。
fn account_of(state: &[MirrorEntry]) -> HashMap<&str, &MirrorEntry> {
    state.iter().map(|entry| (entry.relative_path.as_str(), entry)).collect()
}

fn publish(inner: &Inner, report: MirrorStatus) {
    if let Ok(mut slot) = inner.status.lock() {
        *slot = report;
    }
}

/// 失败的技术说明（进状态给日志味提示，**不是界面文案**）。
fn technical(error: &ApiError) -> String {
    if error.detail.is_empty() {
        error.code.clone()
    } else {
        format!("{} ({})", error.code, error.detail)
    }
}
