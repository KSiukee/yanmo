//! 守护之心：界面**此刻是不是还活着**，以及卡死了怎么自己爬起来。
//!
//! # 为什么需要它
//!
//! 关窗闸门（[`crate::exitwatch`]）解决的是"界面死了窗口还关不掉"；它默认界面**活着**。
//! 反过来的那一半没人管：界面卡死（JS 死循环、页面崩了、WebView 渲染进程被系统回收）而
//! 壳还活着——真机探过：**白板窗口、永不自愈**，作者只能自己去任务管理器杀进程。
//!
//! 这一层补的就是那一半，只看一件事：**界面的心跳还在不在**。心跳停了不代表死透
//! （系统忙、页面在跑一段长任务都可能卡十几秒），所以流程是分段的：
//!
//! ```text
//! 心跳正常 ──超时(15s)──▶ 探活中 ──回话──▶ 心跳正常（误报撤回）
//!                           │
//!                        3s 没回话
//!                           ▼
//!                        判死 ──▶ 留证据 ──▶ 重载 WebView
//!                                                 │
//!                                     心跳回来 ──▶ 心跳正常
//!                                     N 秒没回来 ─▶ 退避后再试（30/60/120s，最多 3 次）
//!                                                      │
//!                                                 用尽/机制失败 ──▶ 停手（把话说清楚，不再动）
//! ```
//!
//! # 三条纪律
//!
//! 1. **先探活，再判死**：超时只是"可能卡了"。壳注入一次探活（要求界面立刻回一个心跳），
//!    回包即撤回——误报要付的代价（丢掉内存里那几个字）远大于多等 3 秒。
//! 2. **绝不无限重载**：一次会话最多 [`MAX_REVIVES`] 次，间隙按 [`BACKOFF`] 退避。
//!    一直重载会让作者看到一个自己闪来闪去的窗口，比白板还糟。
//! 3. **诚实**：重载会丢"上次落盘之后敲的字"（防抖 250ms 起，连续打字时更久）。
//!    这条要说给作者听，不假装无损。
//!
//! # 这一层是**纯逻辑**
//!
//! 不碰 Tauri、不起线程、不看表：`now` 由调用方给（单测拿假时钟把时间拨快），
//! 决定"该做什么"由 [`Watchdog::tick`] 返回 [`Action`]——真去注入 JS / 重载的是壳。
//! 这样子类问题（退避算错、上限失效、误报不撤回）全都能在没有窗口的地方测死。
//!
//! 线程与 Tauri 那一侧见 [`crate::storage`] 的看门狗线程与 `app/src/commands/editor.rs`
//! 的心跳打点。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 多久没心跳算"可能卡了"——**不是判死**，先探活（误报撤回的成本低，判错的成本高）。
pub const STALL: Duration = Duration::from_secs(15);
/// 探活之后等多久回话。正常界面几百毫秒就回来了；超过这个数就是真卡住了。
pub const PROBE_DEADLINE: Duration = Duration::from_secs(3);
/// 一次重载之后等界面重新活过来的宽限（按次数退避）。
pub const BACKOFF: [Duration; 3] =
    [Duration::from_secs(30), Duration::from_secs(60), Duration::from_secs(120)];
/// 一次会话最多自动重载几次——用尽就停手，把结论说清楚。
pub const MAX_REVIVES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// 心跳正常
    Alive,
    /// 超时了：已注入探活，等回话
    Probing { since: Instant },
    /// 刚重载过：等界面重新活过来（`until` 到点还没心跳 = 这次没成）
    Waiting { until: Instant },
    /// 停手了：不再尝试，只留证据等作者处置
    GaveUp,
}

/// 看门狗要壳去做的事（每一步都由 [`Watchdog::tick`] 给出来，壳只管执行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    /// 注入一次探活：要求界面**立刻**回一个心跳
    Probe,
    /// 判死：先留证据，再重载 WebView
    Revive { attempt: u32 },
    /// 停手：把结论告诉作者（同一次会话只报一次）
    GiveUp { attempts: u32 },
}

#[derive(Debug)]
struct Inner {
    /// 最后一次见到界面的时刻（内存打点，**不写库**）
    last_beat: Instant,
    phase: Phase,
    /// 这一次会话已经重载过几次（**只在新会话清零**：免得卡死的界面被无限复活）
    revives: u32,
}

/// 界面心跳 + 状态机（壳持有一个，看门狗线程按秒问它）。
#[derive(Debug)]
pub struct Watchdog {
    inner: Mutex<Inner>,
}

impl Watchdog {
    /// 起一个：从"界面活着"开始算（进程刚起来，界面还在加载——那段时间由冷启动兜底管）。
    pub fn new(now: Instant) -> Self {
        Self {
            inner: Mutex::new(Inner { last_beat: now, phase: Phase::Alive, revives: 0 }),
        }
    }

    /// 见到界面了（任何一次命令调用都算）：推进时间戳，并把"可能要卡了"的判断撤回。
    ///
    /// 撤回是**无条件**的：只要界面对话，之前那次超时就只是它忙了一下——
    /// 包括已经停手之后又活过来（那说明作者自己把它救回来了，我们接着守着）。
    pub fn beat(&self, now: Instant) {
        let mut inner = self.lock();
        inner.last_beat = now;
        inner.phase = Phase::Alive;
    }

    /// 到点了：该做什么？（看门狗线程每秒问一次）
    pub fn tick(&self, now: Instant) -> Action {
        let mut inner = self.lock();
        match inner.phase {
            Phase::Alive => {
                if now.duration_since(inner.last_beat) < STALL {
                    return Action::None;
                }
                // 超时：不判死，先探活
                inner.phase = Phase::Probing { since: now };
                Action::Probe
            }
            // 还在等探活回话：不重复注入
            Phase::Probing { since } if now.duration_since(since) < PROBE_DEADLINE => Action::None,
            Phase::Probing { .. } => revive(&mut inner, now),
            // 刚重载过：宽限期按次数退避
            Phase::Waiting { until } if now < until => Action::None,
            Phase::Waiting { .. } => revive(&mut inner, now),
            Phase::GaveUp => Action::None,
        }
    }

    /// 重载这件事**本身**没成（WebView 已经没了、`reload()` 报错）：直接停手，不再空转。
    ///
    /// 为什么要单独一条：状态机只能看见"心跳没回来"，而"连重载都调不动"是更早、
    /// 更确定的失败——继续按 30/60/120 退避只是在浪费作者的时间。
    pub fn surrender(&self, _now: Instant) -> Action {
        let mut inner = self.lock();
        if matches!(inner.phase, Phase::GaveUp) {
            return Action::None;
        }
        inner.phase = Phase::GaveUp;
        Action::GiveUp { attempts: inner.revives }
    }

    /// 现在处在哪一档（稳定码，进日志与体检——**不是界面文案**）。
    pub fn state(&self) -> &'static str {
        match self.lock().phase {
            Phase::Alive => "alive",
            Phase::Probing { .. } => "probing",
            Phase::Waiting { .. } => "waiting",
            Phase::GaveUp => "gave_up",
        }
    }

    /// 这一次会话已经重载过几次（报告与测试看它）。
    pub fn revives(&self) -> u32 {
        self.lock().revives
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 判死之后的共同一步：还能试就再试一次（带退避），次数用尽就停手。
fn revive(inner: &mut Inner, now: Instant) -> Action {
    if inner.revives >= MAX_REVIVES {
        // 停手只报一次：`GaveUp` 之后再 tick 都返回 None
        if matches!(inner.phase, Phase::GaveUp) {
            return Action::None;
        }
        inner.phase = Phase::GaveUp;
        return Action::GiveUp { attempts: inner.revives };
    }
    inner.revives += 1;
    let index = (inner.revives as usize - 1).min(BACKOFF.len() - 1);
    inner.phase = Phase::Waiting { until: now + BACKOFF[index] };
    Action::Revive { attempt: inner.revives }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 假时钟：**一个基准**往前拨（每次调用都新取 `Instant::now()` 会让基准悄悄漂）。
    fn clock(base: Instant) -> impl Fn(u64) -> Instant {
        move |seconds| base + Duration::from_secs(seconds)
    }

    #[test]
    fn the_limits_themselves_hold_together() {
        assert!(STALL > PROBE_DEADLINE, "探活期限必须短于超时阈值，否则永远来不及");
        assert!(MAX_REVIVES >= 1, "一次都不许重载就没意义了");
        assert!(BACKOFF.windows(2).all(|pair| pair[0] <= pair[1]), "退避要越来越长");
    }

    #[test]
    fn a_healthy_ui_is_left_alone() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        for second in 0..14 {
            assert_eq!(watch.tick(at(second)), Action::None);
            watch.beat(at(second));
        }
        assert_eq!(watch.state(), "alive");
        assert_eq!(watch.revives(), 0);
    }

    #[test]
    fn silence_probes_before_it_condemns() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        assert_eq!(watch.tick(at(14)), Action::None, "还没到阈值");
        assert_eq!(watch.tick(at(15)), Action::Probe, "到阈值：先探活，不判死");
        assert_eq!(watch.state(), "probing");
        assert_eq!(watch.tick(at(16)), Action::None, "探活期内不重复注入");
        assert_eq!(watch.tick(at(17)), Action::None);
    }

    #[test]
    fn an_answer_withdraws_the_false_alarm() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        assert_eq!(watch.tick(at(15)), Action::Probe);
        watch.beat(at(16)); // 界面对话了：刚才只是忙了一下
        assert_eq!(watch.state(), "alive");
        assert_eq!(watch.revives(), 0, "撤回了就不该记成重载过");

        // 计时从**这一次**心跳重新算：不能拿旧时间戳去判下一次
        assert_eq!(watch.tick(at(30)), Action::None, "16 + 15 = 31 之前都还不该动");
        assert_eq!(watch.tick(at(31)), Action::Probe);
    }

    #[test]
    fn no_answer_to_the_probe_is_what_condemns() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        assert_eq!(watch.tick(at(15)), Action::Probe);
        assert_eq!(watch.tick(at(18)), Action::Revive { attempt: 1 }, "3 秒没回话：判死");
        assert_eq!(watch.state(), "waiting");
        assert_eq!(watch.revives(), 1);
    }

    #[test]
    fn a_heartbeat_after_the_reload_means_it_came_back() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        watch.tick(at(15));
        assert_eq!(watch.tick(at(18)), Action::Revive { attempt: 1 });
        watch.beat(at(20)); // 重载后的界面报到了
        assert_eq!(watch.state(), "alive");
        // 救活之后重新按"超时 → 探活"那一段走：再也不会有直接判死
        assert_eq!(watch.tick(at(30)), Action::None, "20 + 15 = 35 之前都还不该动");
        assert_eq!(watch.tick(at(35)), Action::Probe, "再沉默也只是先探活");
        assert_eq!(watch.revives(), 1, "探活阶段不该多记一次重载");
    }

    #[test]
    fn a_reload_that_did_not_work_backs_off_then_gives_up() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        assert_eq!(watch.tick(at(15)), Action::Probe);
        assert_eq!(watch.tick(at(18)), Action::Revive { attempt: 1 });
        // 第 1 次退避 30 秒：到点还没心跳 → 再试
        assert_eq!(watch.tick(at(47)), Action::None, "宽限期内不动");
        assert_eq!(watch.tick(at(48)), Action::Revive { attempt: 2 });
        // 第 2 次退避 60 秒
        assert_eq!(watch.tick(at(100)), Action::None);
        assert_eq!(watch.tick(at(108)), Action::Revive { attempt: 3 });
        // 第 3 次退避 120 秒之后：用尽了，停手
        assert_eq!(watch.tick(at(200)), Action::None);
        assert_eq!(watch.tick(at(228)), Action::GiveUp { attempts: 3 });
        assert_eq!(watch.state(), "gave_up");
        assert_eq!(watch.tick(at(600)), Action::None, "停手只报一次");
    }

    #[test]
    fn after_giving_up_a_live_ui_is_guarded_again_but_not_resurrected_forever() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        watch.tick(at(15));
        watch.tick(at(18));
        watch.tick(at(48));
        watch.tick(at(108));
        assert_eq!(watch.tick(at(228)), Action::GiveUp { attempts: 3 });

        watch.beat(at(230)); // 作者自己把它救回来了
        assert_eq!(watch.state(), "alive");
        assert_eq!(watch.revives(), 3, "这一次会话的次数不清零——不能无限复活");

        // 再卡一次：次数已经在上限，探活之后直接停手，不再重载
        assert_eq!(watch.tick(at(246)), Action::Probe);
        assert_eq!(watch.tick(at(249)), Action::GiveUp { attempts: 3 });
    }

    #[test]
    fn a_reload_that_cannot_even_start_stops_right_there() {
        let at = clock(Instant::now());
        let watch = Watchdog::new(at(0));
        watch.tick(at(15));
        assert_eq!(watch.tick(at(18)), Action::Revive { attempt: 1 });
        assert_eq!(watch.surrender(at(19)), Action::GiveUp { attempts: 1 });
        assert_eq!(watch.state(), "gave_up");
        assert_eq!(watch.surrender(at(20)), Action::None, "停手只报一次");
        assert_eq!(watch.tick(at(600)), Action::None, "停手之后不再空转");
    }

    #[test]
    fn a_busy_ui_is_never_mistaken_for_a_dead_one() {
        let at = clock(Instant::now());
        // 心跳每 10 秒一次（比阈值短）：哪怕连续跑一小时也不该触发任何动作
        let watch = Watchdog::new(at(0));
        for round in 0..360u64 {
            let now = at(round * 10);
            assert_eq!(watch.tick(now), Action::None, "第 {round} 轮");
            watch.beat(now);
        }
        assert_eq!(watch.revives(), 0);
    }
}
