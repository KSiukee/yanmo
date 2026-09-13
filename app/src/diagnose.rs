// i18n-allow-file: 本模块产出的是**诊断日志文件**（给排查看的时间线），不是界面文案。
//! 启动诊断：把"开窗 → 窗口被激活 → 界面就绪 → 输入法有没有挂上来"这条链记成时间线。
//!
//! # 为什么需要它
//!
//! 有一个只在某些 Win10 机器上出现的毛病：窗口起来了、光标也在正文里，但**中文输入法点不出来**，
//! 要切英文打几个字母再切回来才行。焦点代码没改、在别的机器上不复现——这种情况**只能靠现场证据**：
//! 把窗口到底有没有被激活、输入法有没有真的挂到正文元素上，逐条记下来。
//!
//! 之所以写**文件**而不是打印：研墨是 GUI 子系统程序，控制台里没有可用的标准输出
//! （`--check` 那次已经踩过：写了也看不见）。文件不会骗人，脚本再 `type` 出来。
//!
//! 只在 `--diagnose` 下才写；平时这些打点等于空转。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// 进程启动那一刻（时间线从这儿算）。
static STARTED: OnceLock<Instant> = OnceLock::new();
/// 诊断模式开着吗（由 `--diagnose` 打开）。
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// 日志写到哪。
static LOG: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// 认启动参数里的 `--diagnose`（可带 `--out <文件>`）。返回是否开诊断。
pub fn parse(argv: &[String]) -> bool {
    let _ = STARTED.set(Instant::now());
    if !argv.iter().any(|arg| arg == "--diagnose") {
        return false;
    }
    let mut path = std::env::temp_dir().join("yanmo-diagnose.log");
    if let Some(index) = argv.iter().position(|arg| arg == "--out") {
        if let Some(given) = argv.get(index + 1) {
            path = PathBuf::from(given);
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = LOG.set(Mutex::new(path));
    ACTIVE.store(true, Ordering::SeqCst);
    note(&format!(
        "诊断开始（研墨 {}，{} / 逻辑核心 {}）",
        yanmo_core::engine_version(),
        std::env::var("OS").unwrap_or_else(|_| "?".to_string()),
        std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_else(|_| "?".to_string())
    ));
    true
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// 记一条（带"进程启动后第几毫秒"）。没开诊断就什么也不做。
pub fn note(event: &str) {
    if !ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let ms = STARTED.get().map(|started| started.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
    let Some(log) = LOG.get() else { return };
    let Ok(path) = log.lock() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&*path) {
        let _ = writeln!(file, "{ms:>9.1}ms  {event}");
    }
}

/// 页面加载完成：马上要 show + set_focus 了。
pub fn page_loaded() {
    note("页面加载完成：准备 show + set_focus");
}

/// 窗口焦点进来/出去（这是判断"输入法挂没挂上"的第一现场）。
pub fn focus_event(gained: bool) {
    note(if gained { "窗口事件：拿到焦点" } else { "窗口事件：失去焦点" });
}

pub fn resized() {
    note("窗口事件：尺寸变化");
}

pub fn destroyed() {
    note("窗口事件：已销毁");
}

pub fn data_ready() {
    note("数据层已打开");
}

/// 激活重试的一轮：`activated` 表示这一轮看到窗口已经有焦点了。
pub fn activation_round(round: u32, activated: bool) {
    if activated {
        note(&format!("窗口在第 {round} 次查看时已被激活"));
    } else {
        note(&format!("第 {round} 次查看：窗口还没被激活，再要一次焦点"));
    }
}

/// 记一次"窗口现在是什么状态"——**这是整件事的关键证据**：
/// Windows 的输入法是随窗口被激活才挂到输入元素上的，所以"窗口有没有焦点"必须逐次看清。
pub fn note_window(window: &tauri::Window) {
    if !active() {
        return;
    }
    let focused = window.is_focused().unwrap_or(false);
    let visible = window.is_visible().unwrap_or(false);
    let scale = window.scale_factor().unwrap_or(1.0);
    note(&format!("窗口状态：可见={visible} 有焦点={focused} 缩放={scale:.2}"));
}
