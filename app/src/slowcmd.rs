//! 慢命令看门狗：**哪条命令在飞、飞了多久**（守护之心里"慢/卡"的那一半）。
//!
//! # 与 [`crate::watchdog`] 的分工
//!
//! | 那一半 | 判据 | 动作 |
//! |---|---|---|
//! | [`crate::watchdog`] | 界面**心跳没了**（十几秒没人回话） | 探活 → 判死 → 自己把界面救回来 |
//! | 本模块 | 心跳还在，但**一条命令跑了很久** | **只留痕**，一个字都不改行为 |
//!
//! 为什么慢这一半只管留痕：作者等在那儿，界面（多半）也还活着，悄悄替他重试/跳过一条命令
//! 会让他看到"没点过的结果"；而且真正卡住时指望界面自己弹提示是不现实的（它就卡在这条命令上）。
//! 所以：**把现场留下**，出问题时有人能回答"刚才卡在哪儿"。
//!
//! # 量在哪儿
//!
//! 量在**命令边界**（[`crate::commands::registry`] 那一层）：壳上的命令都是同步的，
//! 登记处那一对括号从头到尾框住了它——参数解析、等库锁、核心干活、序列化回包全在里面，
//! 也就是"作者从点下去到看见反应"的那段等待。
//!
//! # 这一层是**纯逻辑**
//!
//! 不碰 Tauri、不起线程、不看表：`now` 由调用方给（单测拿假时钟把时间拨快），
//! 决定"谁慢、慢多久、报过没有"全在这里测死。写库那一半在 [`crate::guardian_wire`]，
//! 节拍在 [`crate::guardian`]。

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 跑过它就留痕：5 秒。作者点一下到看见反应，超过这个数就是"卡了"。
pub const SLOW: Duration = Duration::from_secs(5);
/// 最近这么多条慢命令留着给诊断面板看（条数有界：这是现场，不是账本）。
pub const KEEP: usize = 20;

/// 一条跑完了的慢命令（留着给诊断面板看，也等着落库）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlowSample {
    /// 本进程内的流水号（落库确认按它认，同一条命令慢两次也不会认错）
    pub seq: u64,
    pub command: String,
    pub ms: u64,
}

/// 一条**还在飞**的命令：界面很可能正卡在这条上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inflight {
    pub command: String,
    pub ms: u64,
}

struct Record {
    command: String,
    started_at: Instant,
    /// 已经写进过日志没有（**只报一次**：每拍都报就是刷屏，那份日志谁也读不下去）
    reported: bool,
}

struct Entry {
    sample: SlowSample,
    /// 落库没有（写库那条路可能正被这条慢命令自己占着，见 [`SlowWatch::pending`]）
    persisted: bool,
}

#[derive(Default)]
struct Inner {
    next: u64,
    inflight: HashMap<u64, Record>,
    recent: VecDeque<Entry>,
}

/// 在飞的命令 + 最近的慢命令（壳持有一个，看门狗线程按秒问它）。
#[derive(Default)]
pub struct SlowWatch {
    inner: Mutex<Inner>,
}

impl SlowWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// 一条命令进来了（返回它的牌子，出去时拿它结账）。
    pub fn begin(&self, command: &str, now: Instant) -> u64 {
        let mut inner = self.lock();
        inner.next += 1;
        let token = inner.next;
        inner.inflight.insert(
            token,
            Record { command: command.to_string(), started_at: now, reported: false },
        );
        token
    }

    /// 一条命令跑完了：**太慢就记下来**（返回那条现场，不慢就是 `None`）。
    pub fn end(&self, token: u64, now: Instant) -> Option<SlowSample> {
        let mut inner = self.lock();
        let record = inner.inflight.remove(&token)?;
        let elapsed = now.saturating_duration_since(record.started_at);
        if elapsed < SLOW {
            return None;
        }
        inner.next += 1;
        let sample = SlowSample {
            seq: inner.next,
            command: record.command,
            ms: elapsed.as_millis() as u64,
        };
        inner.recent.push_back(Entry { sample: sample.clone(), persisted: false });
        // 有界：挤掉最老的那条。这是现场不是账本——真正的账在库里的 op-log。
        while inner.recent.len() > KEEP {
            inner.recent.pop_front();
        }
        Some(sample)
    }

    /// 到点了：**还在飞、而且已经超过阈值的**都有谁（每条只报一次）。
    pub fn sweep(&self, now: Instant) -> Vec<Inflight> {
        let mut inner = self.lock();
        let mut slow = Vec::new();
        for record in inner.inflight.values_mut() {
            if record.reported {
                continue;
            }
            let elapsed = now.saturating_duration_since(record.started_at);
            if elapsed < SLOW {
                continue;
            }
            record.reported = true;
            slow.push(Inflight {
                command: record.command.clone(),
                ms: elapsed.as_millis() as u64,
            });
        }
        // 慢的排在前面（报告要一眼看到最慢的那条）
        slow.sort_by(|left, right| right.ms.cmp(&left.ms));
        slow
    }

    /// 最近跑完的慢命令（**新的在前**，给诊断面板看）。
    pub fn recent(&self) -> Vec<SlowSample> {
        self.lock().recent.iter().rev().map(|entry| entry.sample.clone()).collect()
    }

    /// 此刻还在飞的命令（慢的在前）——界面正等在哪一条上。
    pub fn inflight(&self, now: Instant) -> Vec<Inflight> {
        let inner = self.lock();
        let mut flying: Vec<Inflight> = inner
            .inflight
            .values()
            .map(|record| Inflight {
                command: record.command.clone(),
                ms: now.saturating_duration_since(record.started_at).as_millis() as u64,
            })
            .collect();
        flying.sort_by(|left, right| right.ms.cmp(&left.ms));
        flying
    }

    /// **还没落库的**慢命令（旧的在先：照发生顺序写进库）。
    ///
    /// 为什么要这一层"欠着"：写库要过那把库锁，而**慢命令多半正握着它**
    /// （它就是在本进程里干着活）。看门狗那一趟写不进去就先记着，下一趟再写——
    /// 不能因为"这次写不进"就把证据丢了。
    pub fn pending(&self) -> Vec<SlowSample> {
        self.lock()
            .recent
            .iter()
            .filter(|entry| !entry.persisted)
            .map(|entry| entry.sample.clone())
            .collect()
    }

    /// 落库成功：这一条翻成"写过了"（按流水号认，不按命令名——同一条慢两次也不会认错）。
    ///
    /// 已经被挤出留档区的那条（`seq` 找不到）就当它写过了：它本来也只是现场，
    /// 超出的条数自己会被挤掉，不值得为它留着内存。
    pub fn confirm(&self, seq: u64) {
        let mut inner = self.lock();
        if let Some(entry) = inner.recent.iter_mut().find(|entry| entry.sample.seq == seq) {
            entry.persisted = true;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// ── 进程一处的那个实例 ─────────────────────────────────────────────────────
//
// 命令边界（登记处）与看门狗线程是**两条互不相识的路**：前者每条命令都要打点、
// 后者每秒扫一遍。它们共用这一个实例——放全局是刻意的（同 [`crate::diagnose`] 的写法）：
// 这是"这个进程此刻在忙什么"，与哪个窗口、哪本书都无关。

static WATCH: OnceLock<SlowWatch> = OnceLock::new();

fn watch() -> &'static SlowWatch {
    WATCH.get_or_init(SlowWatch::new)
}

/// 一条命令进来了（命令边界调用）。
pub fn begin(command: &str) -> u64 {
    watch().begin(command, Instant::now())
}

/// 一条命令跑完了（命令边界调用）。
pub fn end(token: u64) -> Option<SlowSample> {
    watch().end(token, Instant::now())
}

/// 到点了：还在飞而且太久的都有谁（看门狗线程调用）。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 假时钟：**一个基准**往前拨（每次都新取 `Instant::now()` 会让基准悄悄漂）。
    fn clock(base: Instant) -> impl Fn(u64) -> Instant {
        move |ms| base + Duration::from_millis(ms)
    }

    #[test]
    fn a_quick_command_leaves_nothing_behind() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        let token = watch.begin("save_body", at(0));
        assert_eq!(watch.end(token, at(120)), None, "0.12 秒的落盘不该留痕");
        assert!(watch.recent().is_empty());
        assert!(watch.pending().is_empty(), "没慢过就没有要写库的东西");
        assert!(watch.sweep(at(60_000)).is_empty(), "跑完了就不在飞了");
    }

    #[test]
    fn the_threshold_itself_is_the_line() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        let just_under = watch.begin("compile_work", at(0));
        assert_eq!(watch.end(just_under, at(4_999)), None, "差一点不算");
        let exactly = watch.begin("backup_now", at(0));
        let sample = watch.end(exactly, at(5_000)).expect("正好 5 秒就该记下来");
        assert_eq!(sample.ms, 5_000);
        assert_eq!(sample.command, "backup_now");
    }

    #[test]
    fn an_unknown_token_is_not_a_panic() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        assert_eq!(watch.end(999, at(0)), None, "没这个牌子就当没发生，绝不 panic");
    }

    #[test]
    fn two_commands_in_flight_do_not_confuse_each_other() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        let slow = watch.begin("backup_now", at(0));
        let quick = watch.begin("save_body", at(1_000));
        // 快的那条先出去：不能把慢的那条也结掉
        assert_eq!(watch.end(quick, at(1_100)), None);
        assert_eq!(watch.inflight(at(2_000)).len(), 1);
        assert_eq!(watch.inflight(at(2_000))[0].command, "backup_now");
        let sample = watch.end(slow, at(9_000)).expect("慢的那条还是慢的");
        assert_eq!(sample.command, "backup_now");
        assert_eq!(sample.ms, 9_000);
    }

    #[test]
    fn a_stuck_command_is_reported_once_not_every_tick() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        watch.begin("compile_work", at(0));
        assert!(watch.sweep(at(4_000)).is_empty(), "还没到阈值");
        let first = watch.sweep(at(6_000));
        assert_eq!(first.len(), 1, "到点了要报");
        assert_eq!(first[0].command, "compile_work");
        assert_eq!(first[0].ms, 6_000);
        assert!(watch.sweep(at(7_000)).is_empty(), "**同一条只报一次**：日志不能被它刷屏");
        assert!(watch.sweep(at(30_000)).is_empty(), "一直卡着也不重复报");
    }

    #[test]
    fn sweeps_are_ordered_the_slowest_first() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        watch.begin("save_body", at(0));
        watch.begin("backup_now", at(0));
        let slow = watch.sweep(at(20_000));
        assert_eq!(slow.len(), 2);
        assert_eq!(slow[0].ms, 20_000);
        assert_eq!(slow[1].ms, 20_000, "两条一样久也不该少报一条");
    }

    #[test]
    fn a_finished_command_is_no_longer_swept() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        let token = watch.begin("backup_now", at(0));
        // 在两次扫描之间结束：走 `end` 留痕，不再被扫描报一遍（省得日志与库两边各记一次）
        let sample = watch.end(token, at(7_000)).expect("慢就得记");
        assert_eq!(sample.ms, 7_000);
        assert!(watch.sweep(at(8_000)).is_empty());
        assert_eq!(watch.recent().len(), 1, "记下了，只是不走扫描那条路");
    }

    #[test]
    fn the_recent_list_is_bounded_and_newest_first() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        for round in 0..(KEEP as u64 + 3) {
            let token = watch.begin("backup_now", at(round * 1_000));
            watch.end(token, at(round * 1_000 + 9_000)).unwrap();
        }
        let recent = watch.recent();
        assert_eq!(recent.len(), KEEP, "条数有界：这是现场，不是账本");
        assert!(recent[0].seq > recent[recent.len() - 1].seq, "新的在前");
        assert_eq!(recent[0].ms, 9_000);
    }

    #[test]
    fn evidence_waits_until_it_is_actually_written() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        let token = watch.begin("backup_now", at(0));
        let sample = watch.end(token, at(12_000)).unwrap();

        // 第一次落库没成（库锁正被这条慢命令自己占着）：**还欠着**，下次再写
        assert_eq!(watch.pending(), vec![sample.clone()]);
        watch.confirm(sample.seq);
        assert!(watch.pending().is_empty(), "写进去了就不欠了");
        assert_eq!(watch.recent().len(), 1, "留档区照旧留着给面板看");
    }

    #[test]
    fn confirming_a_sample_that_was_pushed_out_is_harmless() {
        let at = clock(Instant::now());
        let watch = SlowWatch::new();
        let first = {
            let token = watch.begin("backup_now", at(0));
            watch.end(token, at(9_000)).unwrap()
        };
        // 再堆 KEEP 条，把第一条挤出留档区
        for round in 1..=(KEEP as u64) {
            let token = watch.begin("compile_work", at(round * 1_000));
            watch.end(token, at(round * 1_000 + 9_000)).unwrap();
        }
        watch.confirm(first.seq); // 挤掉了：认不出，也不该 panic、不该把别人标掉
        assert_eq!(watch.recent().len(), KEEP);
        assert!(
            watch.pending().iter().all(|sample| sample.command == "compile_work"),
            "认不出就放过，绝不误标别人"
        );
    }
}
