//! 界面**慢**那本账：哪条命令跑了多久、界面自己量到的卡顿有多长。
//!
//! # 与隔壁两本的分工（守护之心的三条线）
//!
//! | 账 | 判据 | 读者 |
//! |---|---|---|
//! | [`super::session`] 会话标记 | 进程活着还是死了（`clean`） | 崩溃恢复 |
//! | [`super::session_revive`] | **界面**在不在（心跳没了） | "要不要给作者一句交代" |
//! | 本文件 | 心跳还在，只是**很久**（命令跑了 12 秒 / 主线程卡了 3 秒） | 事后诊断 |
//!
//! 三条线都归守护之心，但判据与读者都不一样——混在一处，改一条会牵动另外两条。
//!
//! # 判"慢"的机器在壳里，核心只留痕
//!
//! "超过 5 秒算慢""迟到整整一拍算卡"这类判断是**运行期观测**，不属于数据层
//! （核心零 UI 依赖、也不该知道命令名这种东西）。所以：判据在壳（`app/src/slowcmd.rs`
//! 与前端 `editor/drift.ts`），核心只负责把现场**记下来**——写进会话标记与 op-log，
//! **绝不因为"慢"改任何行为**（不重试、不跳过、不提示）。
//!
//! # 为什么也进库，而不只是写日志
//!
//! 与 `ui.revive` 同一个理由：真机走查时诊断日志多半没开，而"刚才为什么卡了一下"这种
//! 事后问题只有库里那份现场能回答。另一条同样实在的理由——**界面被重载过之后，
//! 壳里那份内存记录就没了**（守护之心重载界面正是为了救卡死），而标记里的这一栏还在：
//! 重载回来的界面对着一张写着"上一条慢命令是 X，用了 12 秒"的库，才知道刚才发生了什么。

use super::session::SlowCommand;
use super::Store;
use crate::error::Result;
use crate::time::now_millis;

impl Store {
    /// **一条命令跑得太久**（壳里的慢命令看门狗用）：写进会话标记，并在 op-log 留一条。
    ///
    /// 为什么两样都要：标记回答"**最近**一次是谁"，op-log 回答"一共慢过几次、分别多久"
    /// （append-only，不会被后来那条盖掉）。同事务写，免得一边有一边没有、事后说不清。
    ///
    /// ⚠️ **不推进心跳**：心跳的含义是"界面对话了"，而慢命令恰恰是"界面正等在那儿"。
    /// 顺手推进去，守护之心就会把一条卡住的命令当成界面还活着（误报撤回的前提正是心跳可信）。
    ///
    /// ⚠️ 会话标记还没登记（没走过 `begin_session`）时**不凭空造一个**：那会让
    /// "上次是不是正常退出"这句话失去依据（同 [`Store::note_ui_freeze`]）。
    pub fn note_slow_command(&mut self, command: &str, ms: u64) -> Result<()> {
        let Some(mut marker) = Self::read_marker(&self.conn)? else {
            return Ok(());
        };
        let at = now_millis();
        let node_id = marker.node_id;
        marker.slow = Some(SlowCommand { at, command: command.to_string(), ms });
        let tx = self.conn.transaction()?;
        Self::write_marker(&tx, &marker)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "ui",
            node_id.unwrap_or(0),
            "slow_command",
            serde_json::json!({ "command": command, "ms": ms }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// **界面自己量到的一次卡顿**（1 秒定时器迟了多久）：只追加一条 op-log。
    ///
    /// 为什么不进会话标记：标记是"下一次开窗要读的那几格"，而卡顿是纯粹的事后材料——
    /// 界面上那句读数由界面自己说（它当场就量到了），库里这一条只负责**可查**。
    ///
    /// 为什么不像命令那样带命令名：那一刻主线程卡在什么上，界面并不知道（它连定时器都没跑起来）。
    /// 这是**诚实的空白**——记一个猜出来的名字比不记更坏。
    pub fn note_ui_lag(&mut self, ms: u64) -> Result<()> {
        let node_id = Self::read_marker(&self.conn)?.and_then(|marker| marker.node_id);
        let tx = self.conn.transaction()?;
        Self::record_in(
            &self.device_id,
            &tx,
            "ui",
            node_id.unwrap_or(0),
            "lag",
            serde_json::json!({ "ms": ms }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// **最近一次慢命令**（这一次会话里库记着的那条；没有就是 `None`）。
    ///
    /// 界面重载之后读它：壳里那份内存记录跟着旧页面一起没了，而库里这一栏还在
    /// （同 [`Store::session_revive`] 的处境）。
    pub fn session_slow_command(&self) -> Result<Option<SlowCommand>> {
        Ok(Self::read_marker(&self.conn)?.and_then(|marker| marker.slow))
    }
}
