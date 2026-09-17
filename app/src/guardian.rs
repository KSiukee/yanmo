//! 守护之心的**壳侧**：看门狗线程（探活 → 留证据 → 重载 WebView）。
//!
//! 状态机在 [`crate::watchdog`]（纯逻辑、可假时钟单测）；这里只做三件事：
//! 按秒问它"该做什么"、把 `Action` 变成真动作、把每一步记进体检日志。
//!
//! # 两条接线上的讲究
//!
//! 1. **探活用事件，不用 `eval`**：在死掉的页面上，事件**排着队永远不执行**——
//!    这正好就是我们要的判据（回话＝心跳）。`eval` 还有字符串转义与调试的麻烦。
//! 2. **证据是"尽力而为"**：库里那把锁可能正被一个卡住的界面调用拿着，
//!    所以留证据走 [`AppData::note_ui_freeze_now`]（`try_lock`，不阻塞）——
//!    宁可少一条证据，也不能让看门狗卡在等锁上（它还有更重要的活：把界面救回来）。

use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::storage::AppData;
use crate::watchdog::Action;

/// 判定节拍：一秒看一次。阈值是十几秒，这个精度够用，也几乎不占 CPU。
const STEP: Duration = Duration::from_secs(1);

/// 起线程。**验收模式不起**（那条路的读数要干净，多一个会自动重载的看门狗只会搅乱它）。
pub fn start(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("yanmo-guardian".to_string())
        .spawn(move || run(app));
}

fn run(app: AppHandle) {
    loop {
        std::thread::sleep(STEP);
        // 数据层已经收了（换库那一下）：这一趟就到此为止，别再问
        let Some(data) = app.try_state::<AppData>() else {
            return;
        };
        // 界面还没报到"我就绪了"之前不归看门狗管：冷启动那一段（磁盘慢 / 首帧慢）
        // 可能十几秒没有心跳，那不是在卡死，是在启动。就绪信号复用关窗闸门那一面旗
        // ——"界面已经能回话"是同一件事，不另立一份口径。
        if !data.exit_gate_armed() {
            continue;
        }
        match data.watchdog().tick(Instant::now()) {
            Action::None => {}
            Action::Probe => {
                // 英文：给开发者与体检看，不是界面文案
                crate::diagnose::note("watchdog: no heartbeat, probing the ui");
                probe(&app);
            }
            Action::Revive { attempt } => {
                crate::diagnose::note(&format!(
                    "watchdog: ui looks dead, reloading (attempt {attempt})"
                ));
                data.note_ui_freeze_now(attempt, false);
                if let Err(detail) = reload(&app) {
                    // 连重载都调不动（WebView 已经没了）：停手，别再空转
                    crate::diagnose::note(&format!("watchdog: reload failed: {detail}"));
                    data.note_ui_freeze_now(attempt, true);
                    let _ = data.watchdog().surrender(Instant::now());
                }
            }
            Action::GiveUp { attempts } => {
                crate::diagnose::note(&format!("watchdog: gave up after {attempts} reloads"));
                data.note_ui_freeze_now(attempts, true);
            }
        }
    }
}

/// 探活：请界面**立刻**回一声（回话即心跳，见 `commands::editor::ui_alive`）。
fn probe(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("ui-probe", ());
    }
}

/// 重载 WebView：界面会重新 mount，按会话标记回到原来那一章（恢复到最后落盘的那一版）。
fn reload(app: &AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or_else(|| "no main window".to_string())?;
    window.reload().map_err(|error| error.to_string())
}
