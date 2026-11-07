//! 编辑器会话：**上次是不是正常退出**（崩溃检测 + 会话标记）。
//!
//! 「该编辑哪一章 / 光标读到哪了」已经拆去 [`super::editor_target`]（2026-11-07）：
//! 那是编辑落点与光标，与这里的"上一轮退得干不干净"是两件事，只是共用同一个标记。
//!
//! 本文件围绕 `settings` 里的一个键（JSON）展开：
//!
//! - 启动时写「正在运行」，正常退出时改「已正常退出」；
//! - 启动时发现上一个标记还是「正在运行」→ 上次是被杀 / 崩溃。**这就是崩溃检测的全部依据**：
//!   一个字段，不猜、不靠启发式、不依赖文件时间戳；
//! - 标记里同时记着最后编辑的节点与最后落盘的指纹，因此重开能**回到原来那一章**。
//!
//! ⚠️ 诚实的边界：被杀的那一刻**内存里还没落盘的内容无法恢复**（字只在内存里，
//! 没有任何地方写着它）。本任务保证的是两件不同的事：
//! ① 已经落盘的一个字都不少（WAL + 每次写入一个事务）；
//! ② 关窗 / 失焦 / 切后台前**一定先落盘**，存不下去就不放行。

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::Result;
use crate::time::now_millis;

/// 会话标记在 `settings` 里的键。
const SESSION_KEY: &str = "session.last";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Marker {
    pid: u32,
    started_at: i64,
    heartbeat_at: i64,
    // 这两栏放开给同层（`session_revive` 那本账要用）：标记本身仍只由本文件读写
    pub(super) node_id: Option<i64>,
    fingerprint: String,
    /// 上一次是不是正常退出
    clean: bool,
    /// 界面卡死的一次现场（守护之心留下的交代；没卡过就是 `None`）
    #[serde(default)]
    pub(super) revive: Option<UiFreeze>,
    /// 最近一条跑得太久的命令（慢命令看门狗留下的现场；没慢过就是 `None`）
    #[serde(default)]
    pub(super) slow: Option<SlowCommand>,
}

/// 界面卡死的一次现场：**留给界面的交代**（重载之后要告诉作者发生了什么）。
///
/// 它是"这一次会话里卡过"的记录，不是崩溃检测：卡死时进程还活着，
/// 崩溃检测那个 `clean` 标记此时仍然是"未正常退出"（会话本来就没结束）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiFreeze {
    /// 判死那一刻（unix 毫秒）
    pub at: i64,
    /// 卡住时正在编辑的节点（不知道就是 `None`）
    pub node_id: Option<i64>,
    /// 那一刻库里最后一版的指纹——重载后据此说"恢复到最后落盘的那一版"
    pub fingerprint: String,
    /// 这一次会话里第几次判死
    pub attempt: u32,
    /// 自动重载也没救回来（停手了）：界面下次起来要说得更明白
    pub gave_up: bool,
}

/// 一条**跑得太久**的命令的现场：谁、多久、什么时候。
///
/// 与 [`UiFreeze`] 是两件事：那个说"界面不在了"，这个说"界面还在、只是等了很久"。
/// 判"多久算久"的机器在壳里（命令边界那一对括号），核心只把它记下来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowCommand {
    /// 记下这一刻（unix 毫秒）
    pub at: i64,
    /// 命令名（壳里那份登记处的名字，如 `backup_now`）
    pub command: String,
    /// 这条命令跑了多久
    pub ms: u64,
}

/// 上一次会话留下的交代。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    /// 上一次没有正常退出（被杀 / 崩溃）
    pub unclean: bool,
    /// 崩溃前正在编辑的节点
    pub last_node_id: Option<i64>,
    /// 上次活动时间（unix 毫秒）
    pub last_seen_at: Option<i64>,
    /// 上一次会话里界面卡过没有（卡过就带着现场：次数、那一刻正在编辑的章、最后一版的指纹）
    pub revive: Option<UiFreeze>,
}

impl Store {
    /// 启动时调用一次：读旧标记 → 写新标记 → 报告上次退得干不干净。
    pub fn begin_session(&mut self) -> Result<SessionReport> {
        let report = self.peek_session()?;
        let now = now_millis();
        Self::write_marker(&self.conn, &Marker {
            pid: std::process::id(),
            started_at: now,
            heartbeat_at: now,
            node_id: None,
            fingerprint: String::new(),
            clean: false,
            revive: None,
            slow: None,
        })?;
        Ok(report)
    }

    /// **只读**地看上次会话的交代（不写新标记、不改任何状态）。
    ///
    /// 界面启动走 [`Store::begin_session`]（启动即登记）；而"只想看一眼"的调用方
    /// （命令行工具、体检脚本）走这条——否则看一眼就把上一轮的状态盖掉了。
    pub fn peek_session(&self) -> Result<SessionReport> {
        let previous = Self::read_marker(&self.conn)?;
        Ok(SessionReport {
            unclean: previous.as_ref().map(|m| !m.clean).unwrap_or(false),
            last_node_id: previous.as_ref().and_then(|m| m.node_id),
            last_seen_at: previous.as_ref().map(|m| m.heartbeat_at),
            revive: previous.as_ref().and_then(|m| m.revive.clone()),
        })
    }

    /// 正常退出：**先留关窗快照，再标记干净退出**。返回是否真的写了新快照。
    pub fn end_session(&mut self, node_id: i64) -> Result<bool> {
        let snapshot_written = self.snapshot_if_changed(node_id, "close")?;
        self.mark_clean()?;
        Ok(snapshot_written)
    }

    /// 放弃这次会话（用户选择"仍然退出"）：只标干净，不写快照。
    pub fn abandon_session(&mut self) -> Result<()> {
        self.mark_clean()
    }

    /// 记下"现在打开的是哪一章"。
    ///
    /// 落盘时会自动记，这里补的是**还没写一个字就被杀**的情况——
    /// 否则重开时只知道上次退得不干净，却不知道人当时坐在哪一章。
    pub fn note_open_node(&self, node_id: i64) -> Result<()> {
        self.note_heartbeat(Some(node_id), None)
    }

    /// 心跳：更新活动时间，顺带记住最后编辑的节点与指纹。
    ///
    /// `None` 表示"这一项保持不变"——它由落盘与读回校验顺带调用，不额外增加界面往返。
    pub(super) fn note_heartbeat(&self, node_id: Option<i64>, fingerprint: Option<&str>) -> Result<()> {
        Self::bump(&self.conn, node_id, fingerprint, true)
    }

    /// 同上，但写进**给定的事务**：落盘时"正文 + 留痕 + 心跳"必须同生共死，
    /// 否则心跳写不进去会把已经存好的正文报成"保存失败"（评审：中等 6）。
    pub(super) fn note_heartbeat_in(
        tx: &Transaction<'_>,
        node_id: Option<i64>,
        fingerprint: Option<&str>,
    ) -> Result<()> {
        Self::bump(tx, node_id, fingerprint, true)
    }

    /// 只推进"我还活着"的时间戳，**不碰干净标记**——读回校验（纯粹是读）走这条。
    ///
    /// 为什么要分开（2026-09-15 代码质量评审：中等 11）：关窗时 `end_session` 会标"干净退出"，
    /// 而界面在那之后还可能轮询一次读回校验——以前那次"读"会把 `clean` 标回 false，
    /// 于是下次启动误报"上次没有正常退出"。崩溃提醒是作者唯一的崩溃线索，误报几次就会被忽略。
    pub(super) fn touch(&self) -> Result<()> {
        Self::bump(&self.conn, None, None, false)
    }

    fn bump(
        conn: &Connection,
        node_id: Option<i64>,
        fingerprint: Option<&str>,
        dirty: bool,
    ) -> Result<()> {
        let Some(mut marker) = Self::read_marker(conn)? else {
            return Ok(()); // 没有会话标记（尚未 begin_session）：不凭空造一个
        };
        if let Some(id) = node_id {
            marker.node_id = Some(id);
        }
        if let Some(fp) = fingerprint {
            marker.fingerprint = fp.to_string();
        }
        marker.heartbeat_at = now_millis();
        if dirty {
            // 只有**写路径**才把"干净退出"撤掉：读一下不该被算成还没退干净
            marker.clean = false;
        }
        Self::write_marker(conn, &marker)
    }

    fn mark_clean(&self) -> Result<()> {
        let Some(mut marker) = Self::read_marker(&self.conn)? else {
            return Ok(());
        };
        marker.clean = true;
        // 卡死交代是**给这一次重载看的**：正常退出时清掉，免得下次开窗又念一遍旧事
        marker.revive = None;
        marker.heartbeat_at = now_millis();
        Self::write_marker(&self.conn, &marker)
    }

    /// **界面卡死的证据**（守护之心用）：把这一刻写进会话标记，并在 op-log 留一条。
    ///
    /// 为什么要同事务：这两样是同一件事的两面（"卡过"与"什么时候卡的"），
    /// 一边写进去另一边没写，事后就说不清到底卡没卡。
    ///
    /// `gave_up` 有两个时刻会用：判死那一刻（先按"尝试重载"记，`false`），
    /// 以及重载用尽/调不动时补记一次（`true`）——补记是把同一条交代更新成最终结论。
    ///
    /// ⚠️ 会话标记还没登记（没走过 `begin_session`）时**不凭空造一个**：那会让
    /// "上次是不是正常退出"这句话失去依据。
    pub fn note_ui_freeze(&mut self, attempt: u32, gave_up: bool) -> Result<()> {
        let Some(mut marker) = Self::read_marker(&self.conn)? else {
            return Ok(());
        };
        let at = now_millis();
        let node_id = marker.node_id;
        let fingerprint = marker.fingerprint.clone();
        marker.revive = Some(UiFreeze { at, node_id, fingerprint: fingerprint.clone(), attempt, gave_up });
        marker.heartbeat_at = at;
        let tx = self.conn.transaction()?;
        Self::write_marker(&tx, &marker)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "ui",
            node_id.unwrap_or(0),
            if gave_up { "freeze_gave_up" } else { "freeze" },
            serde_json::json!({
                "attempt": attempt,
                "node_id": node_id,
                "fingerprint": fingerprint,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 读标记。读不出来（没写过 / JSON 坏了）时按**未正常退出**处理——
    /// 宁可多提醒一次，也不要漏报一次崩溃。
    pub(super) fn read_marker(conn: &Connection) -> Result<Option<Marker>> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SESSION_KEY],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|json| {
            serde_json::from_str::<Marker>(&json).unwrap_or(Marker {
                pid: 0,
                started_at: 0,
                heartbeat_at: 0,
                node_id: None,
                fingerprint: String::new(),
                clean: false,
                revive: None,
                slow: None,
            })
        }))
    }

    pub(super) fn write_marker(conn: &Connection, marker: &Marker) -> Result<()> {
        let json = serde_json::to_string(marker).unwrap_or_default();
        conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![SESSION_KEY, json, now_millis()],
        )?;
        Ok(())
    }
}
