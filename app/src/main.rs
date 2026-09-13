//! 研墨桌面壳的**薄入口**。
//!
//! # 壳层纪律
//!
//! - 壳只负责 UI 与转发，**业务逻辑一律写在 `yanmo-core`**；
//! - **击键级同步语义绝不跨进程**——编辑会话留在壳内，只有低频操作过边界；
//! - **数据权威在启动期就交给壳持有**（见 [`storage::AppData`]）：数据目录由壳解析，
//!   前端既不碰文件系统，也没有传路径的入口；
//! - **关窗不放行**：先让界面落盘，存不下去就别想走（闸门在 [`commands::editor`]）；
//!   但界面若已经死了，[`exitwatch`] 会在期限后放行退出——**绝不把窗口锁死**；
//! - 本文件保持极薄：启动 + 注册命令，别在这里长功能。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::{Duration, Instant};

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager};

use crate::exitwatch::RequestOutcome;

mod acceptance;
mod commands;
mod diagnose;
mod error;
mod exitwatch;
mod open_folder;
mod pick_dir;
mod single;
mod storage;
mod volume;

fn main() {
    // **验收模式**（`--self-test-bench` / `--self-test-ui` / `--check`）：只由启动参数进入，
    // 不带参数双击图标的行为一字不变。前两个在独立目录里干活，**绝不碰真实稿库**。
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // **启动诊断**（`--diagnose`）：把"开窗 → 窗口被激活 → 界面就绪 → 输入法有没有挂上"记成时间线。
    // 只在某些 Win10 机器上出现的输入法毛病，靠这份现场证据定位（日志写文件，控制台里看不到输出）。
    diagnose::parse(&argv);
    // `--version`：报版本号，并明说"这一份有没有验收模式"。
    // 为什么要报后者：老版本的 研墨.exe 会把 --self-test-* 当普通参数忽略掉、直接弹窗口干等，
    // 脚本看着就像卡死。让脚本先问一句，老版本就明确停下来（见 tools/acceptance/*.bat）。
    if argv.iter().any(|arg| arg == "--version") {
        // i18n-allow-next-line: 命令行的机器可读输出（给脚本判断用），不是界面文案
        println!("{} acceptance={}", yanmo_core::engine_version(), acceptance::SUPPORTED);
        std::process::exit(0);
    }
    if let Some(plan) = acceptance::parse(&argv) {
        match plan.mode {
            acceptance::Mode::Check => std::process::exit(acceptance::run_check(&plan)),
            acceptance::Mode::Bench => {
                let report = acceptance::run_bench(&plan);
                match acceptance::write_report(&report, &plan.report) {
                    // i18n-allow-next-line: 命令行的机器可读输出（给脚本看），不是界面文案
                    Ok(files) => files.iter().for_each(|path| println!("报告：{}", path.display())),
                    // i18n-allow-next-line: 同上
                    Err(error) => eprintln!("报告没写成：{error}"),
                }
                std::process::exit(0);
            }
            acceptance::Mode::Ui => {
                // 开窗之前记下这一刻：冷启动从这里算起（不含前面造数据的时间）
                acceptance::install_ui(plan);
                acceptance::note_ui_start();
            }
        }
    }
    acceptance::note_command("shell.window_building");
    tauri::Builder::default()
        // **首帧白屏**：窗口先显示、网页还没画出第一帧时，看到的就是 WebView2 的白底。
        // 正解两条一起上：窗口在配置里先隐藏（`visible: false`）+ 底色设成纸色（`backgroundColor`），
        // 这里等页面**加载完成**再把窗口显示出来——用户看到的就是已经画好的界面。
        .on_page_load(|webview, payload| {
            // 验收模式：把"页面加载完成"也记进命令日志（这一步没有，说明卡在页面本身）
            if payload.event() == PageLoadEvent::Finished {
                acceptance::note_command("window.page_loaded");
                acceptance::note_page_loaded();
                let window = webview.window();
                diagnose::page_loaded();
                let _ = window.show();
                // **显示之后还要把它激活**：Windows 的输入法是随"窗口被激活"挂到输入元素上的。
                // 只 show 不激活，Win10 上会出现「中文输入法点不出来、要先切英文打几个字母再切回来」
                // （同样的代码在 Win11 上恰好不露）。这里补一刀，别指望系统替我们做。
                let _ = window.set_focus();
                diagnose::note_window(&window);
                // 有些机器上"激活"这一步会**静默失败**（前台进程切换限制）。给它几次机会：
                // 隔几百毫秒看一眼，没焦点就再要一次（只在前几秒做，之后交给用户）。
                let probe = window.clone();
                std::thread::spawn(move || {
                    for round in 1..=6 {
                        std::thread::sleep(Duration::from_millis(400));
                        let activated = probe.is_focused().unwrap_or(true);
                        diagnose::activation_round(round, activated);
                        if activated {
                            return;
                        }
                        let _ = probe.set_focus();
                    }
                });
            }
        })
        .setup(|app| {
            // 打开失败就让启动失败：半个可用的数据层比不启动更危险。
            // 验收模式走"指定目录"那条路：不读也不写位置记录（验收不许碰真实稿库）。
            let opened = match acceptance::ui_plan() {
                Some(plan) => storage::AppData::open_for_acceptance(&plan.dir),
                None => storage::AppData::open(app.handle()),
            };
            if acceptance::ui_plan().is_some() {
                acceptance::note_command("shell.data_ready");
            }
            diagnose::data_ready();
            let data = match opened {
                Ok(data) => data,
                Err(error) => {
                    // 「这个数据目录已经开着一个研墨了」是唯一要在启动期单独辨出来的失败：
                    // 界面还没起来，得弹一句人话，而不是静默退出（绝不静默失败）。
                    if error.code == "shell.already_running" {
                        let dir = error.params.get("path").cloned().unwrap_or_default();
                        crate::single::announce_already_running(&dir);
                    }
                    return Err(Box::new(error));
                }
            };
            app.manage(data);
            // 验收模式：界面要是到点还没就绪（页面没加载完、前端报错），
            // 也要出报告并退出——**绝不挂在那儿等**。
            if acceptance::ui_plan().is_some() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(acceptance::ui_deadline());
                    acceptance::ui_deadline_passed(&handle);
                });
            }
            // 兜底：万一页面加载完成那个事件没来（前端资源卡住、页面崩了），三秒后也把窗口显示出来
            // ——**绝不因为一个观感优化，把软件变成"点开没反应"**。
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                if let Some(window) = handle.get_webview_window("main") {
                    if !window.is_visible().unwrap_or(true) {
                        let _ = window.show();
                    }
                }
            });
            Ok(())
        })
        // 关窗不直接放行：交给界面先落盘，存不下去就别想走。
        // 闸门未武装（界面还没就绪）时一律放行——否则前端一出问题窗口就关不掉了。
        .on_window_event(|window, event| {
            // 诊断：窗口的焦点进进出出，是判断"输入法挂没挂上"的第一现场
            match event {
                tauri::WindowEvent::Focused(gained) => diagnose::focus_event(*gained),
                tauri::WindowEvent::Resized(_) => diagnose::resized(),
                tauri::WindowEvent::Destroyed => diagnose::destroyed(),
                _ => {}
            }
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let Some(data) = window.app_handle().try_state::<storage::AppData>() else {
                return; // 数据层都没起来：随它关
            };
            if !data.exit_gate_armed() {
                return;
            }
            api.prevent_close();

            match data.exit_watch().request() {
                RequestOutcome::Notified => {
                    let _ = window.emit("close-requested", ());
                    watch_answer_deadline(window.app_handle().clone());
                }
                // 界面没回话、用户又点了一次＝"我就是要关"
                RequestOutcome::Insisted => window.app_handle().exit(0),
                // 界面正在处理（多半弹着"存不下去"的对话框）：这次点击不作数
                RequestOutcome::AlreadyHandling | RequestOutcome::AlreadyExiting => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::engine_info,
            commands::system::diagnose_note,
            commands::system::data_home,
            commands::system::open_data_dir,
            commands::system::exit_app,
            commands::editor::open_editor_target,
            commands::editor::open_chapter,
            commands::editor::open_work_target,
            commands::editor::create_chapter,
            commands::editor::chapter_neighbors,
            commands::editor::save_cursor,
            commands::editor::save_body,
            commands::editor::body_fingerprint,
            commands::editor::emergency_snapshot,
            commands::editor::session_report,
            commands::editor::arm_exit_gate,
            commands::editor::ack_close_request,
            commands::editor::close_session,
            commands::editor::abandon_session,
            commands::editor::escape_export,
            commands::tree::tree_children,
            commands::tree::tree_ancestors,
            commands::tree::tree_create_node,
            commands::tree::tree_rename_node,
            commands::tree::tree_move_node,
            commands::tree::tree_delete_node,
            commands::tree::tree_volume_target,
            commands::tree::tree_set_volume_target,
            commands::gap::tree_gap_check,
            commands::gap::tree_gap_answer,
            commands::gap::tree_fill_gap,
            commands::appearance::appearance_read,
            commands::appearance::appearance_write,
            commands::appearance::appearance_reset,
            commands::snapshot::snapshot_list,
            commands::snapshot::snapshot_diff,
            commands::snapshot::snapshot_keep,
            commands::snapshot::snapshot_drop,
            commands::snapshot::snapshot_restore,
            commands::work::list_shelf,
            commands::work::create_work,
            commands::work::rename_work,
            commands::work::set_work_language,
            commands::backup::backup_status,
            commands::backup::backup_config_write,
            commands::backup::backup_now,
            commands::restore::backup_restore_sources,
            commands::restore::backup_restore_preview,
            commands::restore::backup_restore_apply,
            commands::restore::backup_restore_pick,
            commands::location::data_location_info,
            commands::location::data_location_confirm,
            commands::location::data_location_pick,
            commands::location::data_location_move,
            commands::location::data_location_cancel,
            commands::work::delete_work,
            commands::work::export_work,
            commands::trash::list_trash,
            commands::trash::restore_work,
            commands::trash::restore_preview,
            commands::trash::restore_node,
            commands::trash::purge_node,
            commands::trash::purge_work,
            commands::trash::empty_trash
        ])
        .run(tauri::generate_context!())
        // i18n-allow-next-line: 进程级 panic 文本（开发者看），应用起不来时界面还不存在
        .expect("启动研墨失败");
}

/// 守着"界面有没有回话"的期限。
///
/// 界面若已经死了（WebView 进程被系统回收、页面崩了没人重载），关窗通知就永远没人应答——
/// 这时**必须由壳自己收场**，否则窗口关不掉（实测过：杀 WebView2 后就是这个局面）。
fn watch_answer_deadline(app: AppHandle) {
    std::thread::spawn(move || {
        let step = Duration::from_millis(250);
        loop {
            std::thread::sleep(step);
            let Some(data) = app.try_state::<storage::AppData>() else {
                return;
            };
            if !data.exit_watch().waiting() {
                return; // 界面回话了（或已在退出）：不用再守
            }
            if data.exit_watch().should_force_exit(Instant::now()) {
                app.exit(0);
                return;
            }
        }
    });
}
