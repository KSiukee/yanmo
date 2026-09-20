//! 稿件台**读数**的验收：每一篇一行，状态、规模、在等谁、投过几家都要齐。
//!
//! 看板与书架是**同一批作品**（两处同源），所以这一份也顺带盯着"回收站里的不算、
//! 软删的记录不算"这两条边界。

use yanmo_core::model::{ManuscriptState, WorkKind};
use yanmo_core::store::Store;
use yanmo_core::time::now_millis;

const DAY: i64 = 86_400_000;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn article(store: &mut Store, title: &str, body: &str) -> i64 {
    let work = store.create_work(WorkKind::Article, title).unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(root, body).unwrap();
    work.id
}

/// 看板一行该有的料：状态、字数、在等谁、最近投给谁、投过几家。
#[test]
fn the_board_shows_every_piece_with_its_state_scale_and_pending_venue() {
    let (_dir, mut store) = fresh();
    let now = now_millis();
    let half = article(&mut store, "写了一半", "一二三");
    store.set_work_target_words(half, Some(10_000)).unwrap();
    let sent = article(&mut store, "投出去了", "一二三四五");
    store.record_submission(sent, "甲刊", now, Some(now + 90 * DAY), "").unwrap();

    let board = store.manuscript_board().unwrap();
    assert_eq!(board.len(), 2, "没进回收站的都该在");
    let row = |title: &str| board.iter().find(|r| r.work.title == title).unwrap();

    let half_row = row("写了一半");
    assert_eq!(half_row.work.status, ManuscriptState::Draft);
    assert_eq!(half_row.work.target_words, Some(10_000), "「未完成散篇」比的就是它");
    assert_eq!(half_row.char_count, 3);
    assert_eq!(half_row.chars_no_punct, 3, "界面按字数口径挑一格显示，得跟书架同一份口径");
    assert!(half_row.active.is_none(), "没投过就没有在等的");
    assert_eq!(half_row.submissions, 0);

    let sent_row = row("投出去了");
    assert_eq!(sent_row.work.status, ManuscriptState::Submitted);
    assert_eq!(sent_row.char_count, 5);
    assert_eq!(sent_row.active.as_ref().unwrap().venue, "甲刊");
    assert_eq!(sent_row.last_venue.as_deref(), Some("甲刊"));
    assert_eq!(sent_row.submissions, 1);
}

/// 回收站里的作品不算，软删的投稿记录也不算。
#[test]
fn the_board_leaves_out_trashed_pieces_and_deleted_records() {
    let (_dir, mut store) = fresh();
    let now = now_millis();
    let kept = article(&mut store, "留下的", "正文");
    let trashed = article(&mut store, "扔进回收站的", "正文");
    store.soft_delete_work(trashed).unwrap();

    let sid = store
        .record_submission(kept, "甲刊", now, Some(now + 90 * DAY), "")
        .unwrap()
        .submission
        .id;
    // 先撤回（还在等回音的那条不许删），再删掉这条记录
    store.move_manuscript(kept, ManuscriptState::Ready, "author").unwrap();
    store.delete_submission(sid).unwrap();

    let board = store.manuscript_board().unwrap();
    assert_eq!(board.len(), 1);
    let kept_row = board.iter().find(|r| r.work.title == "留下的").unwrap();
    assert_eq!(kept_row.submissions, 0, "软删的记录不该再算进投过几家");
    assert!(kept_row.active.is_none());
    assert_eq!(kept_row.last_venue, None);
}

/// 目标字数：设、改、清，三样都读得回来（它是「未完成散篇」唯一的料）。
#[test]
fn the_target_word_count_can_be_set_changed_and_cleared() {
    let (_dir, mut store) = fresh();
    let id = article(&mut store, "有目标", "正文");
    assert_eq!(store.work_target_words(id).unwrap(), None, "一开始没设过");

    store.set_work_target_words(id, Some(8_000)).unwrap();
    assert_eq!(store.work_target_words(id).unwrap(), Some(8_000));
    store.set_work_target_words(id, Some(12_000)).unwrap();
    assert_eq!(store.work_target_words(id).unwrap(), Some(12_000), "改就是改，不是累加");

    store.set_work_target_words(id, None).unwrap();
    assert_eq!(store.work_target_words(id).unwrap(), None, "清掉＝回到没设过");
    store.set_work_target_words(id, Some(0)).unwrap();
    assert_eq!(store.work_target_words(id).unwrap(), None, "非正数一样当清掉");
}
