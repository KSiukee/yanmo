//! 删章路标验收：**删掉的章会留下一条可查的路标**，作者点「+」时问一嘴。
//!
//! 四条判据：
//! 1. 要有据才问：被软删、标题能解出号、且那个号**现在没人用**；号被占的不算（那属于恢复同名冲突）；
//! 2. 答复是个三态机：没问过 → 稍后 → 再问一次 → 又稍后 → 自动降为「不用了」；
//! 3. 补写是**新建一个空章**（用原来的名字与位置），旧稿仍留在回收站里；
//! 4. 汇总清单按编号算全书空缺——连没有删除记录的（改名造成的空档）也列出来。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{GapAnswer, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书：一卷三章（都有正文），返回 (作品 id, 卷 id, 三个章的 id)。
fn volume_book(store: &mut Store) -> (i64, i64, Vec<i64>) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for index in 0..3 {
        let id = store
            .create_node(work.id, Some(volume), NodeKind::Chapter, "")
            .unwrap();
        store.write_body(id, &format!("第{}章的正文。", index + 1)).unwrap();
        chapters.push(id);
    }
    (work.id, volume, chapters)
}

/// 这一层现在的顺序（只看活着的节点）。
fn order(store: &Store, work_id: i64, parent_id: i64) -> Vec<i64> {
    store
        .list_nodes(work_id)
        .unwrap()
        .into_iter()
        .filter(|node| node.parent_id == Some(parent_id))
        .map(|node| node.id)
        .collect()
}

#[test]
fn a_deleted_chapter_leaves_a_gap_in_its_own_layer() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = volume_book(&mut store);

    assert!(
        store.gap_in_layer(work, Some(volume)).unwrap().is_none(),
        "没删之前没得问"
    );

    store.soft_delete_node(chapters[1]).unwrap();
    let gap = store.gap_in_layer(work, Some(volume)).unwrap().expect("第2章被删了");

    assert_eq!(gap.node_id, chapters[1], "路标指回回收站里那一条");
    assert_eq!(gap.parent_id, Some(volume), "缺在原来那一卷");
    assert_eq!(gap.serial, 2);
    assert_eq!(gap.title, "第2章");
    assert!(gap.deleted_at > 0, "要知道是什么时候删的");
    assert!(gap.word_count > 0, "要知道旧稿还有多少字");

    // 别的层问不出来：空缺只属于它原来那一层
    assert!(store.gap_in_layer(work, None).unwrap().is_none());
    assert!(store.gap_in_layer(work, Some(chapters[0])).unwrap().is_none());
}

#[test]
fn a_live_chapter_holding_the_number_is_not_a_gap() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = volume_book(&mut store);

    store.soft_delete_node(chapters[1]).unwrap();
    assert!(store.gap_in_layer(work, Some(volume)).unwrap().is_some());

    // 作者自己把第3章改名补上了第2号：这个号不空了，不该再问（那属于恢复时的同名冲突）
    store.rename_node(chapters[2], "第2章").unwrap();
    assert!(
        store.gap_in_layer(work, Some(volume)).unwrap().is_none(),
        "号被占着就不算空缺"
    );
}

#[test]
fn answering_later_asks_once_more_then_stops() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = volume_book(&mut store);
    store.soft_delete_node(chapters[1]).unwrap();

    store.answer_gap(chapters[1], GapAnswer::Deferred).unwrap();
    assert!(
        store.gap_in_layer(work, Some(volume)).unwrap().is_some(),
        "「稍后再说」：下次在同一层点 + 时再问一次"
    );

    store.answer_gap(chapters[1], GapAnswer::Deferred).unwrap();
    assert!(
        store.gap_in_layer(work, Some(volume)).unwrap().is_none(),
        "第二次「稍后」自动降为「不用了」：系统可以问两次，但不该第三次还问"
    );
}

#[test]
fn saying_no_once_stops_the_asking_but_not_the_summary() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = volume_book(&mut store);
    store.soft_delete_node(chapters[1]).unwrap();

    store.answer_gap(chapters[1], GapAnswer::Ignored).unwrap();
    assert!(
        store.gap_in_layer(work, Some(volume)).unwrap().is_none(),
        "「不用了」就不再主动问"
    );

    let all = store.list_gaps(work).unwrap();
    assert!(
        all.iter().any(|gap| gap.serial == 2),
        "但交付前的汇总清单仍要列出这一处：{all:?}"
    );
}

#[test]
fn filling_creates_an_empty_chapter_in_place_and_keeps_the_old_draft() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = volume_book(&mut store);
    store.soft_delete_node(chapters[1]).unwrap();

    let created = store.fill_gap_chapter(chapters[1]).unwrap();
    assert_ne!(created, chapters[1], "补写不是恢复：新建的是另一个节点");
    assert_eq!(store.node_title(created).unwrap(), "第2章", "用原来的名字");
    assert_eq!(
        order(&store, work, volume),
        vec![chapters[0], created, chapters[2]],
        "落回它原来那一带"
    );

    let fresh_chapter = store
        .list_nodes(work)
        .unwrap()
        .into_iter()
        .find(|node| node.id == created)
        .unwrap();
    assert!(!fresh_chapter.has_body, "补出来的是空章，等作者自己写");
    assert_eq!(fresh_chapter.word_count, 0);

    let trash = store.list_trash().unwrap();
    assert!(
        trash.iter().any(|entry| entry.id == chapters[1] && entry.title == "第2章"),
        "旧稿仍在回收站里，想去捞还能捞：{trash:?}"
    );

    // 补完了，这个号不空了，也不再问
    assert!(store.gap_in_layer(work, Some(volume)).unwrap().is_none());
}

#[test]
fn filling_clears_a_previous_later_answer() {
    let (_dir, mut store) = fresh();
    let (work, volume, chapters) = volume_book(&mut store);
    store.soft_delete_node(chapters[1]).unwrap();
    store.answer_gap(chapters[1], GapAnswer::Deferred).unwrap();

    store.fill_gap_chapter(chapters[1]).unwrap();
    assert!(
        store.gap_in_layer(work, Some(volume)).unwrap().is_none(),
        "答复记录随手清掉：这个号不空了"
    );
}

#[test]
fn the_summary_counts_gaps_by_number_even_without_a_deletion_record() {
    let (_dir, mut store) = fresh();
    let (work, _volume, chapters) = volume_book(&mut store);

    // 改个名也能造成空档：第2号没人用了，但回收站里没有对应的删除记录
    store.rename_node(chapters[1], "序章").unwrap();
    let all = store.list_gaps(work).unwrap();
    let hole = all.iter().find(|gap| gap.serial == 2).expect("改名造成的空档也要列出来");
    assert_eq!(hole.node_id, 0, "查不到删除记录：只报「缺第几号」");
    assert_eq!(hole.title, "第2章");
}

#[test]
fn gaps_are_counted_per_layer() {
    let (_dir, mut store) = fresh();
    let (work, volume_one, chapters) = volume_book(&mut store);
    let volume_two = store
        .create_node(work, None, NodeKind::Volume, "第二卷")
        .unwrap();
    store
        .create_node(work, Some(volume_two), NodeKind::Chapter, "")
        .unwrap();

    store.soft_delete_node(chapters[1]).unwrap();
    assert!(
        store.gap_in_layer(work, Some(volume_one)).unwrap().is_some(),
        "第一卷第2章空了"
    );
    assert!(
        store.gap_in_layer(work, Some(volume_two)).unwrap().is_none(),
        "第二卷没缺号，别问错层"
    );
}

#[test]
fn unknown_answers_are_rejected_instead_of_guessed() {
    assert!(GapAnswer::parse("deferred").is_ok());
    assert!(GapAnswer::parse("ignored").is_ok());
    assert!(GapAnswer::parse("maybe").is_err(), "没听过的答复要明确报错");
}
