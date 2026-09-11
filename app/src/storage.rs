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
use yanmo_core::store::{SessionReport, Store};

use crate::exitwatch::ExitWatch;

/// 数据库文件名（位于应用数据目录内）。
const DB_FILE: &str = "yanmo.db";
/// 逃生导出目录（关窗存不下去时，把手上这份正文原子写到这里）。
const ESCAPE_DIR: &str = "escape";

/// 壳持有的数据句柄：核心的存储层句柄（唯一读写入口），加上它落在哪里。
pub struct AppData {
    store: Mutex<Store>,
    db_path: PathBuf,
    session: SessionReport,
    exit_gate_armed: AtomicBool,
    exit_watch: ExitWatch,
}

impl AppData {
    /// 启动期调用一次：解析数据目录 → 建目录 → 打开并迁移数据库 → 登记本次会话。
    ///
    /// 任何一步失败都直接返回错误，**绝不带病启动**（半个可用的数据层比不启动更危险）。
    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("无法确定应用数据目录：{e}"))?;
        Self::open_in(&dir)
    }

    /// 与 [`AppData::open`] 同一条链路，只是目录由调用方给出——供测试直接驱动。
    pub fn open_in(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("无法创建数据目录 {}：{e}", dir.display()))?;
        let db_path = dir.join(DB_FILE);
        let mut store = Store::open(&db_path)
            .map_err(|e| format!("无法打开数据库 {}：{e}", db_path.display()))?;
        // 登记本次会话，同时拿到"上次退得干不干净"的交代
        let session = store
            .begin_session()
            .map_err(|e| format!("无法登记本次会话：{e}"))?;
        Ok(Self {
            store: Mutex::new(store),
            db_path,
            session,
            exit_gate_armed: AtomicBool::new(false),
            exit_watch: ExitWatch::default(),
        })
    }

    /// 数据库文件位置——**只读报告**，界面拿它做展示，拿到也改不了。
    pub fn db_path(&self) -> &Path {
        &self.db_path
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

    /// 在数据句柄上跑一段核心操作。
    ///
    /// 壳只负责**串行化（一把锁）与错误转换**，业务一律在核心：
    /// 这样命令层永远不会长出第二套数据逻辑。
    pub fn with_store<T>(
        &self,
        op: impl FnOnce(&mut Store) -> yanmo_core::Result<T>,
    ) -> Result<T, String> {
        let mut store = self.store.lock().map_err(|_| "数据库句柄不可用".to_string())?;
        op(&mut store).map_err(|e| e.to_string())
    }

    /// 库内实际的数据结构版本（迁移完成后应与引擎期望值一致）。
    pub fn schema_version(&self) -> Result<u32, String> {
        self.with_store(|store| yanmo_core::db::migrations::user_version(store.conn()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_creates_database_where_it_says() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_in(dir.path()).unwrap();
        assert_eq!(data.db_path(), dir.path().join(DB_FILE).as_path());
        assert!(data.db_path().exists(), "启动后数据库文件应当已落盘");
        assert_eq!(
            data.schema_version().unwrap(),
            yanmo_core::db::migrations::schema_version()
        );
    }

    #[test]
    fn reopening_existing_database_keeps_schema() {
        let dir = tempfile::tempdir().unwrap();
        drop(AppData::open_in(dir.path()).unwrap());
        let again = AppData::open_in(dir.path()).unwrap();
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
        let err = match AppData::open_in(&blocker.join("sub")) {
            Ok(_) => panic!("父级是文件时不应打开成功"),
            Err(e) => e,
        };
        assert!(err.contains("无法创建数据目录"), "错误信息应指明失败原因：{err}");
    }

    #[test]
    fn escape_dir_sits_next_to_the_database_and_gate_starts_disarmed() {
        let dir = tempfile::tempdir().unwrap();
        let data = AppData::open_in(dir.path()).unwrap();
        assert_eq!(data.escape_dir(), dir.path().join(ESCAPE_DIR));
        assert!(!data.exit_gate_armed(), "界面没就绪前不该拦关窗");
        data.arm_exit_gate();
        assert!(data.exit_gate_armed());
    }

    #[test]
    fn a_second_launch_reports_the_previous_one_as_unclean() {
        let dir = tempfile::tempdir().unwrap();
        let first = AppData::open_in(dir.path()).unwrap();
        assert!(!first.session().unclean, "第一次启动不该报崩溃");
        drop(first); // 没走正常退出 = 被杀
        let second = AppData::open_in(dir.path()).unwrap();
        assert!(second.session().unclean, "第二次启动必须报出上次是异常退出");
    }
}
