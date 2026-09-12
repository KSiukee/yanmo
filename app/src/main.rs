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

use tauri::{AppHandle, Emitter, Manager};

use crate::exitwatch::RequestOutcome;

mod commands;
mod error;
mod exitwatch;
mod storage;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // 打开失败就让启动失败：半个可用的数据层比不启动更危险。
            let data = storage::AppData::open(app.handle())?;
            app.manage(data);
            Ok(())
        })
        // 关窗不直接放行：交给界面先落盘，存不下去就别想走。
        // 闸门未武装（界面还没就绪）时一律放行——否则前端一出问题窗口就关不掉了。
        .on_window_event(|window, event| {
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
            commands::system::data_home,
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
