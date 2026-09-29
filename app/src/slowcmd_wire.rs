//! 慢命令看门狗的**壳侧接线**：进程一处的那份现场，以及"拿到就得记账"的计时器。
//!
//! # 为什么与 [`crate::slowcmd`]（判据那一半）分开
//!
//! 那一半是**纯逻辑**（谁慢、慢多久、报过没有、欠着没写库的怎么补），一个全局变量都没有，
//! 所以能拿假时钟把每种情形测死；这一份是"**这个进程此刻在忙什么**"那唯一一份实例——
//! 命令边界（登记处）与看门狗线程是两条互不相识的路，只有它俩共用同一份才说得通。
//!
//! 放全局是刻意的（同 [`crate::diagnose`] 的写法）：它描述的是进程状态，与哪个窗口、
//! 哪本书都无关。分开写的另一个实在理由：那一份单测很多，混在一起会把那个文件顶到
//! 上帝线上（仓库里有一条"≥400 行只许减不许增"的棘轮），而这一份根本不需要单测。

use std::sync::OnceLock;
use std::time::Instant;

use crate::slowcmd::{Inflight, SlowSample, SlowWatch};

/// 进程一处的那份。
static WATCH: OnceLock<SlowWatch> = OnceLock::new();

fn watch() -> &'static SlowWatch {
    WATCH.get_or_init(SlowWatch::new)
}

/// 一条命令的计时器：**出去时自动结账**。
///
/// 为什么做成守卫而不是"进去 begin、出来 end"两句：命令里万一 panic（展开），
/// 那一句 end 就轮不到跑——那条命令会永远挂在"还在飞"里，看门狗每趟都把它算进去。
/// 守卫由 Drop 收尾，panic 展开也一样收干净（同 [`crate::exitwatch`] 那种"绝不留下
/// 半开状态"的写法）。
#[must_use = "a dropped timer measures nothing: bind it (let _timer = ...) instead of writing \
              a throwaway expression"]
pub struct Timer {
    token: u64,
}

impl Drop for Timer {
    fn drop(&mut self) {
        let _ = watch().end(self.token, Instant::now());
    }
}

/// 一条命令进来了（命令边界调用）：拿一个计时器，它在离开作用域时自己结账。
pub fn start(command: &str) -> Timer {
    Timer { token: watch().begin(command, Instant::now()) }
}

/// 到点了：还在飞而且太久的都有谁（看门狗线程调用，每条只报一次）。
pub fn sweep() -> Vec<Inflight> {
    watch().sweep(Instant::now())
}

/// 此刻还在飞的命令（诊断面板调用）。
pub fn inflight() -> Vec<Inflight> {
    watch().inflight(Instant::now())
}

/// 最近跑完的慢命令（新的在前）。
pub fn recent() -> Vec<SlowSample> {
    watch().recent()
}

/// 还没落库的慢命令（旧的在先）。
pub fn pending() -> Vec<SlowSample> {
    watch().pending()
}

/// 落库成功，那条翻成写过了。
pub fn confirm(seq: u64) {
    watch().confirm(seq)
}

/// 还欠着库、而且这一笔还没说过一次的（看门狗线程用它写日志，每笔只说一次）。
pub fn pending_to_note() -> Vec<SlowSample> {
    watch().pending_to_note()
}

/// 这一笔的"还欠着"说过了（说过不再重复报）。
pub fn note_deferral(seq: u64) {
    watch().note_deferral(seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 守卫那条路：**离开作用域就结账**——命令里 panic（展开）也一样收干净，
    /// 不留"永远挂在还在飞"的账（那会让看门狗每趟都把它算一遍）。
    #[test]
    fn the_timer_closes_the_account_when_it_goes_out_of_scope() {
        assert!(inflight().is_empty(), "这之前没有在飞的命令");

        let timer = start("probe");
        assert_eq!(inflight().len(), 1, "计上之后就该看得见它在飞");
        drop(timer);
        assert!(inflight().is_empty(), "守卫一出去就该结账");
        assert!(pending().is_empty(), "刚进去就出来，不该留账");
    }
}
