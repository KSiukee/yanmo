//! 投稿**记录账**的验收：撤稿、晚来的回信、冷冻期、改与删。
//!
//! 这些是"投过哪几家、还在等谁、哪天可改投"真正要回答的问题——所以每一条都从库里
//! 读回来对账，而不是只看返回值。

use yanmo_core::error_codes::codes;
use yanmo_core::model::{ManuscriptState, SubmissionResult, WorkKind};
use yanmo_core::store::Store;
use yanmo_core::time::now_millis;

const DAY: i64 = 86_400_000;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn article(store: &mut Store, title: &str) -> i64 {
    let work = store.create_work(WorkKind::Article, title).unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(root, "正文").unwrap();
    work.id
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

/// 投资一篇（默认甲刊、90 天冷冻期）。
fn send(store: &mut Store, work_id: i64, venue: &str, now: i64) -> i64 {
    store.record_submission(work_id, venue, now, Some(now + 90 * DAY), "").unwrap().submission.id
}

/// 还在等回音时：判词拦得住（软拦），撤稿之后同一条路就通了。
#[test]
fn withdrawing_frees_the_piece_for_another_venue() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "投一篇");
    let now = now_millis();
    send(&mut store, id, "甲刊", now);

    let blocked = store.submission_verdict_for(id, "乙刊").unwrap();
    assert_eq!(blocked.code(), "awaiting_reply", "还在等甲刊的回音");

    store.move_manuscript(id, ManuscriptState::Ready, "author").unwrap();
    let history = store.submissions(id).unwrap();
    assert_eq!(history[0].result, SubmissionResult::Withdrawn, "撤稿要留下这一笔");
    assert!(store.submission_verdict_for(id, "乙刊").unwrap().is_clear(), "撤回之后不该再拦别家");

    let receipt = store.record_submission(id, "乙刊", now, Some(now + 90 * DAY), "").unwrap();
    assert_eq!(receipt.from, ManuscriptState::Ready);
    assert_eq!(store.submissions(id).unwrap().len(), 2, "改投是第二笔账，不是覆盖第一笔");
}

/// 撤稿（或超期先标回待投）之后编辑才回信：**改的是那条记录**，不是新记一笔。
#[test]
fn a_late_accept_lands_on_the_withdrawn_record() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "晚来的回信");
    let now = now_millis();
    let sid = send(&mut store, id, "甲刊", now);
    store.move_manuscript(id, ManuscriptState::Ready, "author").unwrap();

    store.move_manuscript(id, ManuscriptState::Accepted, "author").unwrap();
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Accepted);
    let history = store.submissions(id).unwrap();
    assert_eq!(history.len(), 1, "同一笔投稿，不该变成两笔");
    assert_eq!(history[0].result, SubmissionResult::Accepted);
    assert_eq!(sub_ops(&store, sid), vec!["record", "withdrawn", "accepted"], "整条线索都在 op-log 里");
}

/// 被拒之后：同刊冷冻期未满会被劝一句；换一家可以直接投；回炉与改投两条路都通。
#[test]
fn a_rejected_piece_can_move_on() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "被拒了");
    let now = now_millis();
    send(&mut store, id, "甲刊", now);
    store.move_manuscript(id, ManuscriptState::Rejected, "author").unwrap();

    assert_eq!(store.submission_verdict_for(id, "甲刊").unwrap().code(), "venue_cooldown");
    assert!(store.submission_verdict_for(id, "乙刊").unwrap().is_clear(), "换一家不必等");

    // 重出路径：被拒 → 待投 → 再投（改投别家）
    store.move_manuscript(id, ManuscriptState::Ready, "author").unwrap();
    let again = store.record_submission(id, "乙刊", now, Some(now + 90 * DAY), "").unwrap();
    assert_eq!(again.from, ManuscriptState::Ready);
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Submitted);
    assert_eq!(store.submissions(id).unwrap().len(), 2);
}

/// 冷冻期过了再投同一家：不再算一稿多投，但仍提醒"这家投过"。
#[test]
fn an_overdue_submission_stops_blocking_but_is_still_remembered() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "等到超期");
    let now = now_millis();
    // 冷冻期设在过去：编辑没回音，视为可改投
    store.record_submission(id, "甲刊", now - 200 * DAY, Some(now - DAY), "").unwrap();

    assert!(store.submission_verdict_for(id, "乙刊").unwrap().is_clear());
    assert_eq!(store.submission_verdict_for(id, "甲刊").unwrap().code(), "already_tried");
}

/// 改一条记录：改哪几样就落哪几样，并留一条 `update`。
#[test]
fn editing_a_record_is_traced() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "记错了");
    let now = now_millis();
    let sid = send(&mut store, id, "甲刊", now);

    store.update_submission(sid, "甲刊（副刊）", now - DAY, None, "改过目标").unwrap();
    let history = store.submissions(id).unwrap();
    assert_eq!(history[0].venue, "甲刊（副刊）");
    assert_eq!(history[0].submitted_at, now - DAY);
    assert_eq!(history[0].cooldown_until, None, "清掉期限就是清掉");
    assert_eq!(history[0].notes, "改过目标");
    assert_eq!(sub_ops(&store, sid), vec!["record", "update"]);

    let missing = store.update_submission(999, "甲刊", now, None, "").unwrap_err();
    assert_eq!(missing.code(), codes::SUBMISSION_NOT_FOUND);
}

/// 还在等回音的那条不许删（先撤稿再删）；删掉之后它不再参与判词。
#[test]
fn a_waiting_record_cannot_be_deleted_until_the_piece_is_withdrawn() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "删记录");
    let now = now_millis();
    let sid = send(&mut store, id, "甲刊", now);

    let in_use = store.delete_submission(sid).unwrap_err();
    assert_eq!(in_use.code(), codes::SUBMISSION_IN_USE);
    assert_eq!(store.submissions(id).unwrap().len(), 1, "拒绝时不该真的删掉");

    store.move_manuscript(id, ManuscriptState::Ready, "author").unwrap();
    store.delete_submission(sid).unwrap();
    assert!(store.submissions(id).unwrap().is_empty(), "软删的不算历史");
    assert!(store.submission_verdict_for(id, "甲刊").unwrap().is_clear(), "删掉之后不该再拦这家");
    assert_eq!(sub_ops(&store, sid), vec!["record", "withdrawn", "delete"]);
}

/// 空目标不是"投给谁"——拒绝，并且状态不动。
#[test]
fn an_empty_venue_is_refused() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "没写目标");
    let now = now_millis();

    let err = store.record_submission(id, "   ", now, None, "").unwrap_err();
    assert_eq!(err.code(), codes::SUBMISSION_VENUE_EMPTY);
    assert_eq!(store.manuscript_status(id).unwrap(), ManuscriptState::Draft);
    assert!(store.submissions(id).unwrap().is_empty());
}
