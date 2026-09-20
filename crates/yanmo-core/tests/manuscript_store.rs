//! 稿件**状态落库**的验收：七态各自一条真实路径、每一步都留得下证据、说不通的一步不许写。
//!
//! 这是"状态可达性"的**落库那一半**（纯逻辑那一半在 `tests/manuscript.rs`）：
//! 路径上的每一步都用真 `Store` 走一遍，然后**从库里读回来对账**——状态字段、
//! 投稿记录、op-log 三样都要对得上。

use yanmo_core::error_codes::codes;
use yanmo_core::model::{ManuscriptState, WorkKind};
use yanmo_core::store::Store;
use yanmo_core::time::now_millis;

const DAY: i64 = 86_400_000;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 建一篇有字数的短篇（短篇的根节点就是正文本身）。
fn article(store: &mut Store, title: &str, body: &str) -> i64 {
    let work = store.create_work(WorkKind::Article, title).unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(root, body).unwrap();
    work.id
}

/// 这本书在 op-log 里的动作码（按发生顺序）。
fn work_ops(store: &Store, work_id: i64) -> Vec<String> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op FROM op_log WHERE entity = 'works' AND entity_id = ?1 ORDER BY seq")
        .unwrap();
    let rows = stmt.query_map([work_id], |r| r.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// 这条投稿记录在 op-log 里的动作码（按发生顺序）。
fn sub_ops(store: &Store, submission_id: i64) -> Vec<String> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op FROM op_log WHERE entity = 'submissions' AND entity_id = ?1 ORDER BY seq")
        .unwrap();
    let rows = stmt.query_map([submission_id], |r| r.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// 七个态各自一条从草稿出发的真实路径，落库走一遍：状态读得回来、动作码留得下来。
#[test]
fn every_state_is_reachable_through_the_store() {
    let (_dir, mut store) = fresh();
    let now = now_millis();
    let until = Some(now + 90 * DAY);
    let send = |store: &mut Store, title: &str| {
        let id = article(store, title, "正文");
        store.record_submission(id, "甲刊", now, until, "").unwrap();
        id
    };

    let draft = article(&mut store, "草稿", "正文");
    let finished = article(&mut store, "完稿", "正文");
    store.move_manuscript(finished, ManuscriptState::Finished, "author").unwrap();
    let ready = article(&mut store, "待投", "正文");
    store.move_manuscript(ready, ManuscriptState::Ready, "author").unwrap();
    let submitted = send(&mut store, "已投");
    let accepted = send(&mut store, "过稿");
    store.move_manuscript(accepted, ManuscriptState::Accepted, "author").unwrap();
    let rejected = send(&mut store, "被拒");
    store.move_manuscript(rejected, ManuscriptState::Rejected, "author").unwrap();
    let published = send(&mut store, "已发");
    store.move_manuscript(published, ManuscriptState::Accepted, "author").unwrap();
    store.move_manuscript(published, ManuscriptState::Published, "author").unwrap();

    use ManuscriptState::*;
    for (id, state) in [
        (draft, Draft),
        (finished, Finished),
        (ready, Ready),
        (submitted, Submitted),
        (accepted, Accepted),
        (rejected, Rejected),
        (published, Published),
    ] {
        assert_eq!(store.manuscript_status(id).unwrap(), state, "{} 的状态没落对", state.as_str());
    }
    // 每一步都得留下**可观测的证据**：op-log 里那个动作码
    for (id, action) in [
        (finished, "finish"),
        (ready, "ready"),
        (submitted, "submit"),
        (accepted, "accept"),
        (rejected, "reject"),
        (published, "publish"),
    ] {
        assert!(work_ops(&store, id).contains(&action.to_string()), "op-log 里没有 {action}");
    }
}

/// 记一次投稿：历史、状态、留痕三样一个事务里一起落。
#[test]
fn recording_a_submission_writes_history_status_and_trace() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "投一篇", "正文");
    let now = now_millis();
    let until = now + 90 * DAY;

    let receipt = store.record_submission(id, " 甲刊 ", now, Some(until), " 附了简介 ").unwrap();
    assert_eq!(receipt.from, ManuscriptState::Draft, "补记：从草稿直接投出去也允许");
    assert_eq!(receipt.to, ManuscriptState::Submitted);
    assert!(receipt.verdict.is_clear(), "第一次投，没有拦的理由");

    let history = store.submissions(id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].venue, "甲刊", "目标两头去空白");
    assert_eq!(history[0].result, yanmo_core::model::SubmissionResult::Waiting);
    assert_eq!(history[0].cooldown_until, Some(until));
    assert_eq!(history[0].notes, "附了简介");
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Submitted);
    assert!(work_ops(&store, id).contains(&"submit".to_string()));
    assert_eq!(sub_ops(&store, history[0].id), vec!["record"]);
}

/// 说不通的一步：**一个字节都不写**（状态、历史、op-log 三样都不动）。
#[test]
fn an_illegal_transition_writes_nothing() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "草稿", "正文");
    let before = work_ops(&store, id).len();

    let err = store.move_manuscript(id, ManuscriptState::Published, "author").unwrap_err();
    assert_eq!(err.code(), codes::MANUSCRIPT_ILLEGAL_TRANSITION);
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Draft);
    assert_eq!(work_ops(&store, id).len(), before, "拒绝的这一步不该留痕");
}

/// 进「已投」必须记一次投稿——没有投向与时刻的「已投」是空账。
#[test]
fn moving_into_submitted_without_a_record_is_refused() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "草稿", "正文");

    let err = store.move_manuscript(id, ManuscriptState::Submitted, "author").unwrap_err();
    assert_eq!(err.code(), codes::MANUSCRIPT_SUBMIT_NEEDS_RECORD);
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Draft);
}

/// 没投过就标「过稿」：拒绝，并说清是"没有可结的记录"。
#[test]
fn accepting_a_piece_that_was_never_submitted_is_refused() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "没投过", "正文");
    store.move_manuscript(id, ManuscriptState::Ready, "author").unwrap();

    let err = store.move_manuscript(id, ManuscriptState::Accepted, "author").unwrap_err();
    assert_eq!(err.code(), codes::MANUSCRIPT_NO_OPEN_SUBMISSION);
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Ready, "拒绝时不落半截状态");
}

/// 关掉再打开：状态与投稿历史都还在（v17 的列与表是真落盘的）。
#[test]
fn status_and_history_survive_closing_and_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let now = now_millis();
    let id = {
        let mut store = Store::open(&path).unwrap();
        let id = article(&mut store, "跨重启", "正文");
        store.record_submission(id, "甲刊", now, Some(now + 90 * DAY), "").unwrap();
        store.move_manuscript(id, ManuscriptState::Rejected, "author").unwrap();
        id
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Rejected);
    assert_eq!(store.submissions(id).unwrap().len(), 1);
    assert_eq!(store.submissions(id).unwrap()[0].result, yanmo_core::model::SubmissionResult::Rejected);
}

/// 已经在 v17 上的库把结构版本退回 16 再打开：重跑迁移不炸、数据一字不动。
///
/// （升级路径的"幂等"这一半；另一条"老库里的稿子都算草稿"由 v17 自己的 `prepare` 单测盯着。）
#[test]
fn migration_seventeen_is_idempotent_on_an_existing_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let now = now_millis();
    let id = {
        let mut store = Store::open(&path).unwrap();
        let id = article(&mut store, "老稿子", "正文");
        store.record_submission(id, "甲刊", now, None, "").unwrap();
        store.conn().pragma_update(None, "user_version", 16).unwrap();
        id
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Submitted);
    assert_eq!(store.submissions(id).unwrap().len(), 1);
}
