//! 通信决策引擎验收：**有界、可取消、会断路、崩溃有预算，而且从不等待**。
//!
//! 这组测的是纯状态机（时间由测试给，不睡真时钟）；真正的 IPC / 进程管理在宿主那一层。

use yanmo_core::error_codes::codes;
use yanmo_core::module_comm::{
    Admission, CallPlace, CallResult, CallToken, CommEvent, CommPolicy, CrashDecision,
    FailureKind, HealthState, ModuleComm, RejectReason,
};

const MODULE: &str = "app.example.map";

fn policy() -> CommPolicy {
    CommPolicy {
        max_in_flight: 1,
        max_queued: 1,
        deadline_ms: 1_000,
        max_response_bytes: 1_024,
        failure_threshold: 2,
        open_cooldown_ms: 5_000,
        crash_limit: 2,
        crash_window_ms: 10_000,
    }
}

fn comm() -> ModuleComm {
    ModuleComm::new(policy()).unwrap()
}

fn token(admission: Admission) -> CallToken {
    match admission {
        Admission::Started(token) | Admission::Queued(token) => token,
        Admission::Rejected(reason) => panic!("应当收下，却被拒：{}", reason.as_str()),
    }
}

fn started(comm: &mut ModuleComm, now: i64) -> CallToken {
    token(comm.try_start(MODULE, now))
}

fn started_on(comm: &mut ModuleComm, module_id: &str, now: i64) -> CallToken {
    token(comm.try_start(module_id, now))
}

fn rejected(admission: Admission) -> RejectReason {
    match admission {
        Admission::Rejected(reason) => reason,
        other => panic!("应当拒绝：{other:?}"),
    }
}

#[test]
fn in_flight_and_queue_are_bounded() {
    let mut comm = comm();
    let first = token(comm.try_start(MODULE, 0));
    let second = token(comm.try_start(MODULE, 0));
    assert_ne!(first, second);
    assert_eq!(rejected(comm.try_start(MODULE, 0)), RejectReason::QueueFull);

    let health = comm.health(MODULE, 0);
    assert_eq!(health.in_flight, 1);
    assert_eq!(health.queued, 1);
    assert_eq!(health.state, HealthState::Healthy);

    // 不排队（max_queued=0）时，第二个直接拒
    let mut no_queue = ModuleComm::new(CommPolicy { max_queued: 0, ..policy() }).unwrap();
    started(&mut no_queue, 0);
    assert_eq!(rejected(no_queue.try_start(MODULE, 0)), RejectReason::QueueFull);
}

#[test]
fn a_completed_call_promotes_the_next_one() {
    let mut comm = comm();
    let first = started(&mut comm, 0);
    let second = token(comm.try_start(MODULE, 0));

    let effect = comm.complete(MODULE, first, CallResult::Ok { bytes: 10 }, 100);
    assert!(effect.accepted);
    assert_eq!(effect.failure, None);
    assert_eq!(effect.next, Some(second));
    assert!(effect.expired.is_empty());

    let health = comm.health(MODULE, 100);
    assert_eq!(health.in_flight, 1, "排队的那一个被晋升了");
    assert_eq!(health.queued, 0);

    // 晋升后的截止时间仍按**入队那一刻**算：1001 就过期了
    let events = comm.expire(1_001);
    assert!(matches!(
        events.as_slice(),
        [CommEvent::TimedOut { token, place: CallPlace::InFlight, .. }] if *token == second
    ));
}

#[test]
fn cancelling_queued_and_in_flight_frees_the_slot() {
    let mut comm = comm();
    let first = started(&mut comm, 0);
    let queued = token(comm.try_start(MODULE, 0));

    let cancel = comm.cancel(MODULE, queued, 10);
    assert!(cancel.cancelled);
    assert_eq!(cancel.next, None);
    assert_eq!(comm.health(MODULE, 10).queued, 0);

    let next = token(comm.try_start(MODULE, 20));
    let cancel = comm.cancel(MODULE, first, 30);
    assert!(cancel.cancelled);
    assert_eq!(cancel.next, Some(next), "取消在途要立刻把名额让给排队者");
    assert_eq!(comm.health(MODULE, 30).in_flight, 1);

    let unknown = started_on(&mut comm, "app.example.other", 0);
    assert!(!comm.cancel(MODULE, unknown, 40).cancelled);
    assert!(!comm.complete(MODULE, unknown, CallResult::Ok { bytes: 1 }, 40).accepted);
}

#[test]
fn expire_times_out_in_flight_and_promotes_a_live_queue_entry() {
    let mut comm = comm();
    let first = started(&mut comm, 0); // deadline 1000
    let second = token(comm.try_start(MODULE, 500)); // deadline 1500

    let events = comm.expire(1_001);
    assert!(events.iter().any(|event| matches!(
        event,
        CommEvent::TimedOut { token, place: CallPlace::InFlight, .. } if *token == first
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CommEvent::Started { token, .. } if *token == second
    )), "排队里还没过期的应当被晋升");

    let events = comm.expire(1_501);
    assert!(events.iter().any(|event| matches!(
        event,
        CommEvent::TimedOut { token, place: CallPlace::InFlight, .. } if *token == second
    )));
}

#[test]
fn the_breaker_opens_and_half_opens() {
    let mut comm = comm();
    let first = started(&mut comm, 0);
    let effect = comm.complete(MODULE, first, CallResult::Failed, 10);
    assert_eq!(effect.failure, Some(FailureKind::Error));
    assert_eq!(comm.health(MODULE, 10).state, HealthState::Degraded);

    let second = started(&mut comm, 20);
    comm.complete(MODULE, second, CallResult::Failed, 30);
    assert_eq!(comm.health(MODULE, 30).state, HealthState::Open);
    assert_eq!(rejected(comm.try_start(MODULE, 100)), RejectReason::CircuitOpen);

    // 冷却结束：允许试探；成功就关回去
    assert_eq!(
        comm.health(MODULE, 5_030).state,
        HealthState::HalfOpen,
        "冷却后只是半开，探测成功才真正恢复"
    );
    let probe = started(&mut comm, 5_030);
    comm.complete(MODULE, probe, CallResult::Ok { bytes: 1 }, 5_031);
    assert_eq!(comm.health(MODULE, 5_031).state, HealthState::Healthy);
}

#[test]
fn oversize_responses_are_failures_not_successes() {
    let mut comm = comm();
    let first = started(&mut comm, 0);
    let effect = comm.complete(MODULE, first, CallResult::Ok { bytes: 2_048 }, 10);
    assert_eq!(effect.failure, Some(FailureKind::Oversize));
    assert_eq!(comm.health(MODULE, 10).state, HealthState::Degraded);

    let second = started(&mut comm, 20);
    comm.complete(MODULE, second, CallResult::Ok { bytes: 2_048 }, 30);
    assert_eq!(comm.health(MODULE, 30).state, HealthState::Open);
}

#[test]
fn crash_budget_restarts_twice_then_stops() {
    let mut comm = comm();
    assert_eq!(comm.record_crash(MODULE, 0), CrashDecision::Restart { attempt: 1 });
    assert_eq!(comm.record_crash(MODULE, 100), CrashDecision::Restart { attempt: 2 });
    assert_eq!(comm.record_crash(MODULE, 200), CrashDecision::Stopped { crashes: 3 });
    assert_eq!(comm.health(MODULE, 200).state, HealthState::Stopped);
    assert_eq!(rejected(comm.try_start(MODULE, 201)), RejectReason::Stopped);

    // 窗口外的旧崩溃不算数
    let mut comm = ModuleComm::new(CommPolicy { crash_window_ms: 1_000, ..policy() }).unwrap();
    assert_eq!(comm.record_crash(MODULE, 0), CrashDecision::Restart { attempt: 1 });
    assert_eq!(
        comm.record_crash(MODULE, 5_000),
        CrashDecision::Restart { attempt: 1 },
        "五秒前那次已经出了窗口"
    );
}

#[test]
fn a_crash_clears_in_flight_and_stopping_clears_the_queue() {
    let mut comm = comm();
    started(&mut comm, 0);
    token(comm.try_start(MODULE, 0));
    assert_eq!(comm.record_crash(MODULE, 10), CrashDecision::Restart { attempt: 1 });
    let health = comm.health(MODULE, 10);
    assert_eq!(health.in_flight, 0, "崩掉的调用已经没了");
    assert_eq!(health.queued, 1, "重启时排队保留");

    comm.record_crash(MODULE, 20);
    comm.record_crash(MODULE, 30);
    let health = comm.health(MODULE, 30);
    assert_eq!(health.state, HealthState::Stopped);
    assert_eq!(health.queued, 0, "停用要连排队一起清");
}

#[test]
fn reporting_a_crash_from_a_call_uses_the_budget() {
    let mut comm = comm();
    let token = started(&mut comm, 0);
    let effect = comm.complete(MODULE, token, CallResult::Crashed, 10);
    assert_eq!(effect.failure, Some(FailureKind::Crashed));
    assert_eq!(effect.crash, Some(CrashDecision::Restart { attempt: 1 }));
    assert_eq!(comm.health(MODULE, 10).in_flight, 0);
}

#[test]
fn policy_validation_rejects_zeroes() {
    let bad = [
        CommPolicy { max_in_flight: 0, ..policy() },
        CommPolicy { deadline_ms: 0, ..policy() },
        CommPolicy { max_response_bytes: 0, ..policy() },
        CommPolicy { failure_threshold: 0, ..policy() },
        CommPolicy { crash_window_ms: 0, ..policy() },
    ];
    for policy in bad {
        assert_eq!(
            ModuleComm::new(policy).unwrap_err().code(),
            codes::MODULE_COMM_POLICY_INVALID,
            "策略应当被拒：{policy:?}"
        );
    }
    // 不排队是合法的
    assert!(ModuleComm::new(CommPolicy { max_queued: 0, ..policy() }).is_ok());
}

#[test]
fn unknown_modules_are_safe_and_healthy() {
    let comm = comm();
    assert_eq!(comm.health("app.example.none", 0).state, HealthState::Healthy);
    assert_eq!(comm.health("app.example.none", 0).in_flight, 0);
}
