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
/// 导出目录名：优先落在作者的"文档"里（他自己找得到的地方），拿不到就退回数据目录。
const EXPORT_FOLDER: &str = "研墨导出";
const EXPORT_DIR: &str = "export";

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
) -> Result<usize, String> {
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
        let entries = std::fs::read_dir(&current)
            .map_err(|e| format!("读导出目录 {} 失败：{e}", current.display()))?;
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
                std::fs::remove_file(&path)
                    .map_err(|e| format!("清旧导出文件 {} 失败：{e}", path.display()))?;
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
pub struct AppData {
    store: Mutex<Store>,
    db_path: PathBuf,
    /// 导出根目录（**路径策略在壳**：界面既不选路径也不碰文件系统）
    export_dir: PathBuf,
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
        // 导出放"文档/研墨导出"：那是作者自己找得到的地方；系统答不上来就退回数据目录
        let export_dir = app
            .path()
            .document_dir()
            .map(|home| home.join(EXPORT_FOLDER))
            .unwrap_or_else(|_| dir.join(EXPORT_DIR));
        Self::open_at(&dir, export_dir)
    }

    /// 目录由调用方给出——供测试直接驱动。
    #[cfg(test)]
    fn open_at_for_test(dir: &Path) -> Result<Self, String> {
        Self::open_at(dir, dir.join(EXPORT_DIR))
    }

    fn open_at(dir: &Path, export_dir: PathBuf) -> Result<Self, String> {
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
            export_dir,
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
    ) -> Result<ExportOutcome, String> {
        let dir = self
            .export_dir
            .join(yanmo_core::atomic::safe_file_name(work_title));
        for file in files {
            let path = dir.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("无法创建导出目录 {}：{e}", parent.display()))?;
            }
            let unchanged = std::fs::read_to_string(&path)
                .map(|old| old == file.content)
                .unwrap_or(false);
            if unchanged {
                continue;
            }
            yanmo_core::atomic::write_atomic(&path, file.content.as_bytes())
                .map_err(|e| format!("写导出文件 {} 失败：{e}", path.display()))?;
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
        assert!(err.contains("无法创建数据目录"), "错误信息应指明失败原因：{err}");
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
}
