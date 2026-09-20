//! 守护之心的**壳侧**：看门狗线程（探活 → 留证据 → 重载 WebView / 精准杀渲染进程）。
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
                    "watchdog: ui looks dead, pulling a lever (attempt {attempt})"
                ));
                data.note_ui_freeze_now(attempt, false);
                let done = revive(&app, attempt);
                // 英文：给开发者与体检看，不是界面文案
                crate::diagnose::note(&format!(
                    "watchdog: attempt {attempt} used {} ({})",
                    done.lever, done.detail
                ));
                // 哪一刀动的手也进库：真机演练正是靠这条才看得出"第二刀到底有没有动手"
                data.note_ui_revive_now(attempt, done.lever, &done.detail, done.alive);
                if !done.alive {
                    // 连这一根杠杆都调不动（窗口已经没了）：停手，别再空转
                    crate::diagnose::note("watchdog: lever could not be pulled; giving up");
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

/// 第一根杠杆：软重载。界面会重新 mount，按会话标记回到原来那一章。
///
/// ⚠️ **真机实测：它破不开 JS 死循环**（命令连回执都没有）——死循环占着渲染进程，
/// 重载这一步排在那儿永远轮不到。所以它只是第一根，后面还有两根（见 [`revive`]）。
fn reload(app: &AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or_else(|| "no main window".to_string())?;
    window.reload().map_err(|error| error.to_string())
}

/// 第几次判死用哪根杠杆。
///
/// # 剩下这几根都试过了（2026-09-17 真机演练：往真页面注入一个死循环，再从外面看反应）
///
/// | 手段 | 真机结果 |
/// |---|---|
/// | 软重载（本函数） | ❌ 破不开死循环：命令连回执都没有（那个渲染进程正忙着转圈） |
/// | 硬导航（`WebView2.Navigate` 同类） | ❌ 命令被接受，页面 10 秒内仍不回话 |
/// | **整扇窗口重建**（销毁 + 按配置再建一个） | ❌ **更糟**：窗口重建了、页面却再没挂起来 |
/// | CDP `Page.crash`（想让浏览器自己换渲染进程） | ❌ 命令发不出去（它要经由那个卡住的页面） |
/// | 杀掉浏览器下面**所有**子进程 | ❌ 反而救不回来（GPU / 网络那些也被捎带杀了） |
/// | **只杀 `--type=renderer` 那一个** | ✅ **10 秒内页面就回来了**（浏览器自己拉起新渲染进程） |
///
/// 结论：**有效的那一根是"精准挑出渲染进程并杀掉"**——它已经补上了，见
/// [`crate::renderer`]（挑的规则，纯逻辑可测）与 [`crate::renderer_win`]（Windows 那一半：
/// 捞进程树 / 读命令行 / 杀那几个 pid）。用的是现成的 `windows-sys`，没有新增依赖。
///
/// 阶梯：**第一次软重载**（最便宜，也不动别的东西——页面卡在别的原因上时它就够用）；
/// **第二次起"杀渲染进程 + 紧接着重载"**（唯一能破开 JS 死循环的那一下；
/// 重载必须跟着——只杀掉的话那棵树里会连 renderer 都没有，页面要等下一次导航才回来，实测过）。
/// 挑不出来（进程已经没了 / 命令行读不出来）就**退回软重载**，绝不"按名字乱杀"。
///
/// 回来的这一份**要进库**（`ui.revive` 那条 op-log）：哪一刀、成没成、`alive` 是真是假——
/// 只写在日志里的话，真机演练那种"日志没开"的场次就什么都查不到。
fn revive(app: &AppHandle, attempt: u32) -> Attempt {
    if attempt <= 1 {
        return lever_result("reload", reload(app));
    }
    match crate::renderer_win::kill_renderers() {
        Ok(detail) => {
            // **杀掉之后要紧接着重载**——真机读数（2026-09-20）：只杀不重载，页面不会自己回来
            // （杀掉之后那棵树里连一个 renderer 都没有了，直到下一次导航才重新拉起；
            // 上一轮"第三刀挑不出来、退回重载"反而把页面带回来，就是这么回事）。
            // 两下连着做，也把复活时间从 112 秒压回一个退避周期。
            let (detail, alive) = match reload(app) {
                Ok(()) => (format!("{detail}; reloaded"), true),
                Err(also) => (format!("{detail}; reload failed: {also}"), false),
            };
            Attempt { lever: LEVER_KILL, detail, alive }
        }
        Err(detail) => {
            // 挑不出来：退回最便宜的那根——它拉得动就算这一刀还有救
            let (detail, alive) = match reload(app) {
                Ok(()) => (format!("{detail}; fell back to reload"), true),
                Err(also) => (format!("{detail}; reload also failed: {also}"), false),
            };
            Attempt { lever: LEVER_KILL, detail, alive }
        }
    }
}

/// 「精准杀渲染进程」这一根的名字（进 op-log；**别改**，演练与体检按它认）。
const LEVER_KILL: &str = "renderer-kill";

/// 这一刀的结果：用了哪根杠杆、留一句给日志与 op-log 的话、以及**界面还有没有指望**。
struct Attempt {
    lever: &'static str,
    detail: String,
    /// `false` = 这根杠杆根本拉不动（窗口已经没了）：调用方据此停手，别再空转
    alive: bool,
}

/// 「软重载」这一根的结果。
fn lever_result(lever: &'static str, outcome: Result<(), String>) -> Attempt {
    match outcome {
        Ok(()) => Attempt { lever, detail: format!("{lever} pulled"), alive: true },
        Err(detail) => Attempt { lever, detail, alive: false },
    }
}

