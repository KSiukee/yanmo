//! 会话与退出闸门一族的命令：**界面报到了没有、卡过没有、关窗放不放行**。
//!
//! 与 [`crate::commands::editor`]（边写边存：打开 / 落盘 / 指纹 / 抢救）分开的原因：
//! 那一族的读者是"编辑器"，这一族的读者是**壳自己**——心跳（守护之心）、
//! 上次会话的交代、关窗闸门的回话与收尾。两边的变化理由不一样。
//!
//! 一条纪律照旧：**探活那两条不碰库**（`ui_alive` / `body_fingerprint`）——
//! 否则"核心卡住"会被误判成"界面卡死"（库锁正被一个卡住的界面调用拿着，正是那种场景）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;

/// 上一次会话的交代。
#[derive(Debug, Serialize)]
pub struct SessionNotice {
    /// 上次没有正常退出（被杀 / 崩溃）
    pub unclean: bool,
    /// 崩溃前正在编辑的节点
    pub last_node_id: Option<i64>,
    /// 上次活动时间（unix 毫秒）
    pub last_seen_at: Option<i64>,
    /// 上次会话里界面卡过没有（卡过就带着现场：第几次、卡在哪一章、最后一版的指纹）
    pub revive: Option<yanmo_core::store::UiFreeze>,
    /// 守护之心此刻的状态（`alive` / `probing` / `waiting` / `gave_up`）——体检与演练读它
    pub watchdog: String,
    /// 这一次会话已经自动重载过几次
    pub revives: u32,
}

/// 退出收尾的回执。
#[derive(Debug, Serialize)]
pub struct CloseAck {
    /// 是否真的留了关窗快照（内容没变就不留，避免堆垃圾）
    pub snapshot_written: bool,
}

/// 探活的回话（见 [`crate::guardian`]）：**不碰库**——否则"核心卡住"会被误判成"界面卡死"。
#[tauri::command]
pub fn ui_alive(data: State<'_, AppData>) {
    data.watchdog().beat(std::time::Instant::now());
}

/// 上次会话的交代：界面启动时问一次，决定要不要提示"上次没有正常退出"。
#[tauri::command]
pub fn session_report(data: State<'_, AppData>) -> SessionNotice {
    let session = data.session();
    // 现读那次卡死的交代（看门狗在这一次会话中途写的）；读不到就退回启动时那份快照
    let revive = data.live_revive().or_else(|| session.revive.clone());
    SessionNotice {
        unclean: session.unclean,
        last_node_id: session.last_node_id,
        last_seen_at: session.last_seen_at,
        revive,
        // 守护之心的现状一并报出来：真机演练与体检据此看"判了没有、重载几次"
        watchdog: data.watchdog().state().to_string(),
        revives: data.watchdog().revives(),
    }
}

/// 界面已就绪——从现在起关窗会先过闸门。
#[tauri::command]
pub fn arm_exit_gate(app: tauri::AppHandle, data: State<'_, AppData>) {
    crate::acceptance::note_command("ui.ready");
    data.arm_exit_gate();
    // 验收模式：界面刚就绪——把"冷启动到可用"这一刻量下来，写完报告壳自己退出
    crate::acceptance::ui_ready(&app);
}

/// 界面回话：收到关窗通知，开始处理（落盘，或弹"存不下去"的对话框）。
///
/// 壳据此停掉"界面是不是已经死了"的倒计时——**拦住关窗是界面故意的，而不是界面没反应**。
#[tauri::command]
pub fn ack_close_request(data: State<'_, AppData>) {
    data.exit_watch().acknowledge();
}

/// 正常退出前收尾：**留一份关窗快照 + 标记干净退出**。
#[tauri::command(rename_all = "snake_case")]
pub fn close_session(data: State<'_, AppData>, node_id: i64) -> Result<CloseAck, ApiError> {
    data.with_store(|store| {
        Ok(CloseAck {
            snapshot_written: store.end_session(node_id)?,
        })
    })
}

/// 用户选择"仍然退出"：只标记干净退出，不写快照（把选择权交还给人）。
#[tauri::command]
pub fn abandon_session(data: State<'_, AppData>) -> Result<(), ApiError> {
    data.with_store(|store| store.abandon_session())
}
