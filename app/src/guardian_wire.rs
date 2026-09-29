//! 守护之心在**壳侧的那点接线**：看门狗线程要用到的 `AppData` 口子。
//!
//! 四件小事，都是"有则更好"的账——**一律走非阻塞入口**（[`AppData::try_with_store`]）：
//! 宁可少一条痕，也不能让看门狗卡在等库锁上（它还有更重要的活：把界面救回来）。
//!
//! 为什么单独成件：它们是守护之心这条线的读写口，跟着 [`crate::guardian`] 一起变；
//! 而 [`crate::storage`] 是**组合根**（数据目录 / 单实例 / 搬迁 / 会话 / 退出闸门 / 镜像），
//! 每加一个运行期持有的东西才在那儿接一次线——把一个看门狗的口子塞进组合根，
//! 只会让那个本来就很大的文件继续长。
//!
//! 底下两笔账在核心：`note_ui_freeze`（卡死现场，进会话标记 + 流水）与
//! `note_ui_revive`（哪一刀动的手，只进流水）。

use yanmo_core::store::{SlowCommand, UiFreeze};

use crate::storage::AppData;

impl AppData {
    /// **界面卡死的证据**：写进会话标记 + 一条流水。
    pub fn note_ui_freeze_now(&self, attempt: u32, gave_up: bool) {
        match self.try_with_store(|store| store.note_ui_freeze(attempt, gave_up)) {
            Some(Ok(())) => {}
            // 英文：给开发者与体检看，不是界面文案
            Some(Err(_)) => crate::diagnose::note("watchdog: freeze evidence write failed"),
            None => crate::diagnose::note("watchdog: freeze evidence skipped (store busy or closed)"),
        }
    }

    /// **这一刀用了哪根杠杆、成没成**（排查用；真机演练靠它看"第二刀有没有动手"）。
    pub fn note_ui_revive_now(&self, attempt: u32, lever: &str, detail: &str, alive: bool) {
        match self.try_with_store(|store| store.note_ui_revive(attempt, lever, detail, alive)) {
            Some(Ok(())) => {}
            Some(Err(_)) => crate::diagnose::note("watchdog: revive record write failed"),
            None => crate::diagnose::note("watchdog: revive record skipped (store busy or closed)"),
        }
    }

    /// **一条命令跑得太久**（慢命令看门狗）：写进会话标记 + 一条流水。
    ///
    /// 返回**真的写进去了没有**：写不进去（库锁正被这条慢命令自己占着）时调用方要**下一拍再试**——
    /// 现场丢了就再也补不回来，而这正是"事后为什么卡了一下"唯一的答案。
    pub fn note_slow_command_now(&self, command: &str, ms: u64) -> bool {
        match self.try_with_store(|store| store.note_slow_command(command, ms)) {
            Some(Ok(())) => true,
            Some(Err(_)) => {
                // 英文：给开发者与体检看，不是界面文案
                crate::diagnose::note("slow command: evidence write failed");
                false
            }
            None => false,
        }
    }

    /// **界面自己量到的一次卡顿**（1 秒定时器迟了多久）：只追加一条流水。
    ///
    /// 界面上那句读数由界面自己说（它当场就量到了）；库里这一条只负责**事后可查**。
    pub fn note_ui_lag_now(&self, ms: u64) {
        match self.try_with_store(|store| store.note_ui_lag(ms)) {
            Some(Ok(())) => {}
            Some(Err(_)) => crate::diagnose::note("ui stall: evidence write failed"),
            None => crate::diagnose::note("ui stall: evidence skipped (store busy or closed)"),
        }
    }

    /// **这次会话里界面刚卡过没有**——重载回来的界面靠它拿到那句交代。
    ///
    /// 为什么现读而不是用启动时那份快照：卡死与复活发生在**这一次会话中途**，
    /// 快照里永远是空的（真机演练逮到过：复活成功了，屏幕上却没那句话）。

    /// **库里记着的最近一条慢命令**（设置里那块"运行诊断"读它）。
    ///
    /// 为什么现读：界面被重载过之后，壳里那份内存记录跟着旧页面一起没了
    /// （[`crate::slowcmd`] 活在新页面里，前面那几条不在），而库里这一栏还在。
    pub fn live_slow_command(&self) -> Option<SlowCommand> {
        self.try_with_store(|store| store.session_slow_command().ok().flatten()).flatten()
    }
    pub fn live_revive(&self) -> Option<UiFreeze> {
        self.try_with_store(|store| store.session_revive().ok().flatten()).flatten()
    }
}
