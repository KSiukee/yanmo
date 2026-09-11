//! 书架验收：**多作品不是附加功能，是默认形态**。
//!
//! 三条判据（就是"不绑死一本书"的落地）：
//! 1. 书架一眼看全书——书名之外还有章数与字数，且顺序是最近打开优先；
//! 2. **每本书各记各的"读到哪了"**——切回来能秒回原位，不会被另一本书覆盖；
//! 3. 「当前作品」不许做成全局单例：切走再切回来，位置与光标都得回得来。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{EditorCursor, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn cursor(anchor: i64) -> EditorCursor {
    EditorCursor {
        anchor,
        head: anchor,
        scroll_top: anchor * 10,
    }
}

/// 一本书里按顺序建几章，正文写进去（字数才有意义）。
fn add_chapter(store: &mut Store, work_id: i64, parent: Option<i64>, title: &str, body: &str) -> i64 {
    let id = store.create_node(work_id, parent, NodeKind::Chapter, title).unwrap();
    store.write_body(id, body).unwrap();
    id
}

#[test]
fn shelf_shows_every_book_with_its_size() {
    let (_dir, mut store) = fresh();
    let long = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(long.id).unwrap()[0].id;
    add_chapter(&mut store, long.id, Some(volume), "第一章", "一二三四五");
    add_chapter(&mut store, long.id, Some(volume), "第二章", "六七");

    let article = store.create_work(WorkKind::Article, "短记").unwrap();
    let piece = store.list_nodes(article.id).unwrap()[0].id;
    store.write_body(piece, "随笔一句。").unwrap();

    let shelf = store.shelf().unwrap();
    assert_eq!(shelf.len(), 2, "两本书都要在书架上");
    assert_eq!(shelf[0].work.id, article.id, "刚建的排在最前（最近打开优先）");

    let long_entry = shelf.iter().find(|e| e.work.id == long.id).unwrap();
    assert_eq!(long_entry.chapters, 2, "长篇两章");
    assert_eq!(long_entry.word_count, 7, "5 + 2，用的是预聚合字数");
    assert_eq!(long_entry.work.title, "长夜");

    let article_entry = shelf.iter().find(|e| e.work.id == article.id).unwrap();
    assert_eq!(article_entry.chapters, 0, "单篇文章不是「章」，前端只报字数就好");
    assert!(article_entry.word_count > 0);

    // 软删的书不再出现在书架上
    store.soft_delete_work(article.id).unwrap();
    let after = store.shelf().unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].work.id, long.id);
}

#[test]
fn every_book_remembers_its_own_cursor() {
    let (_dir, mut store) = fresh();
    let one = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let two = store.create_work(WorkKind::Novel, "短歌").unwrap();
    let one_chapter = add_chapter(&mut store, one.id, None, "第一章", "正文");
    let two_chapter = add_chapter(&mut store, two.id, None, "开篇", "正文");

    store.save_cursor(one_chapter, cursor(11)).unwrap();
    store.save_cursor(two_chapter, cursor(22)).unwrap();

    // ★ 关键：在第二本书里记位置，**不能**把第一本书的位置冲掉
    assert_eq!(store.load_cursor(one_chapter).unwrap(), Some(cursor(11)));
    assert_eq!(store.load_cursor(two_chapter).unwrap(), Some(cursor(22)));

    // 光标只认自己那章：拿另一章去问，得不到别人的位置
    assert_eq!(store.load_cursor(one.id).unwrap(), None);
}

#[test]
fn switching_books_returns_to_the_chapter_you_left() {
    let (_dir, mut store) = fresh();
    let one = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let two = store.create_work(WorkKind::Novel, "短歌").unwrap();
    add_chapter(&mut store, one.id, None, "第一章", "正文");
    let one_second = add_chapter(&mut store, one.id, None, "第二章", "正文");
    let two_first = add_chapter(&mut store, two.id, None, "开篇", "正文");

    // 今天在《长夜》写到第二章
    store.save_cursor(one_second, cursor(30)).unwrap();

    // 明天换《短歌》：落点是它的第一章（这本书还没有记录）
    let switched = store.work_target(two.id).unwrap();
    assert_eq!(switched.work_id, two.id);
    assert_eq!(switched.node_id, two_first);
    store.save_cursor(two_first, cursor(5)).unwrap();

    // 再切回《长夜》：**回到第二章，位置也在**（这就是"秒回原位"）
    let back = store.work_target(one.id).unwrap();
    assert_eq!(back.node_id, one_second);
    assert_eq!(store.load_cursor(back.node_id).unwrap(), Some(cursor(30)));

    // 切回去之后再回《短歌》，它的位置也没被《长夜》冲掉
    let again = store.work_target(two.id).unwrap();
    assert_eq!(again.node_id, two_first);
    assert_eq!(store.load_cursor(again.node_id).unwrap(), Some(cursor(5)));
}

#[test]
fn work_target_falls_back_when_there_is_no_memory() {
    let (_dir, mut store) = fresh();
    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(novel.id).unwrap()[0].id;

    // 只建了一卷、还没写一个字：切过去要能立刻落笔（补一章），不能让人对着空目录发呆
    let target = store.work_target(novel.id).unwrap();
    assert_eq!(target.work_id, novel.id);
    assert_eq!(target.title, "第一章");
    let nodes = store.list_nodes(novel.id).unwrap();
    assert!(nodes.iter().any(|n| n.id == target.node_id && n.parent_id == Some(volume)));

    // 已经有一章了：落点就是它
    let chapter = add_chapter(&mut store, novel.id, Some(volume), "引子", "正文");
    store.save_cursor(chapter, cursor(3)).unwrap();
    assert_eq!(store.work_target(novel.id).unwrap().node_id, chapter);

    // 那一章被删了：不报错，退回这本书里还在的第一章
    let other = add_chapter(&mut store, novel.id, Some(volume), "另一章", "正文");
    store.save_cursor(other, cursor(4)).unwrap();
    store.soft_delete_node(other).unwrap();
    let fallback = store.work_target(novel.id).unwrap();
    assert_ne!(fallback.node_id, other, "删掉的章不能当落点");
    assert_eq!(store.work_target(novel.id).unwrap().node_id, fallback.node_id);
}

#[test]
fn legacy_single_slot_cursor_is_honored_then_moved() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let chapter = add_chapter(&mut store, work.id, None, "第一章", "正文");

    // 装成"老库"：只有全局单槽那条记录
    let legacy = r#"INSERT OR REPLACE INTO settings(key, value, updated_at)
                    VALUES('editor.cursor', '{"node_id":NODE,"cursor":{"anchor":7,"head":7,"scroll_top":70}}', 0)"#
        .replace("NODE", &chapter.to_string());
    store.conn().execute(&legacy, []).unwrap();

    // 升级后照样认得出"上次读到哪了"
    assert_eq!(store.load_cursor(chapter).unwrap(), Some(cursor(7)));

    // 写一次新记录：单槽那条就该退场（否则每次都要兜一次，越兜越乱）
    store.save_cursor(chapter, cursor(9)).unwrap();
    let left: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM settings WHERE key = 'editor.cursor'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0, "分键记录写上了，旧单槽要清掉");
    assert_eq!(store.load_cursor(chapter).unwrap(), Some(cursor(9)));
}
