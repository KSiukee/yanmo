//! 出场人物验收：**节点 ↔ 设定卡**这条关联的读、写、与"删了会怎样"。
//!
//! 五件事：
//! 1. 挂上去能在每一行读回来、名字是卡上的名字（按名字排）；
//! 2. 整份覆盖（去掉一个、加一个）——不是两套加减接口；
//! 3. 卡改名 → 关联不动，读出来是新名（关联认 id）；
//! 4. 卡软删 → 从每一章的人名里消失（不留脏）；
//! 5. 别人的卡 / 卷 / 不存在的段，一律当场拒（一个字都不写）。

use yanmo_core::error::codes;
use yanmo_core::model::{EntityKind, NewEntityCard, NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn person(store: &mut Store, work_id: i64, name: &str) -> i64 {
    store
        .create_entity_card(
            &NewEntityCard {
                work_id,
                kind: EntityKind::Person,
                name: name.to_string(),
                aliases: Vec::new(),
                attributes: Vec::new(),
                note: String::new(),
            },
            "test",
        )
        .unwrap()
}

/// 一本书 + 一卷下的两章。
fn book_with_two_chapters(store: &mut Store) -> (i64, i64, i64, i64) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    (work.id, volume, first, second)
}

fn op_log_count(store: &Store) -> i64 {
    store
        .conn()
        .query_row("SELECT COUNT(*) FROM op_log WHERE entity = 'node_cast'", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn cast_round_trips_covers_and_follows_the_card_name() {
    let (_dir, mut store) = fresh();
    let (work, volume, first, second) = book_with_two_chapters(&mut store);
    let lu = person(&mut store, work, "陆文");
    let zhang = person(&mut store, work, "老张");

    // 挂两个人：读回来按名字排
    let saved = store.set_node_cast(first, &[lu, zhang], "test").unwrap();
    assert_eq!(saved.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["老张", "陆文"]);

    let rows = store.outline_rows(work).unwrap();
    let cast_len = |id: i64| rows.iter().find(|r| r.node_id == id).unwrap().cast.len();
    assert_eq!(cast_len(first), 2);
    assert_eq!(cast_len(second), 0, "没挂过人的章是空名单");
    assert_eq!(cast_len(volume), 0, "卷是分组行，没有名单这一说");

    // 整份覆盖：去掉老张、只留陆文；重复点同一个人只算一次
    let saved = store.set_node_cast(first, &[lu, lu], "test").unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].entity_id, lu);

    // 卡改名：关联不动，读出来是新名
    let card = store.entity_card(lu).unwrap();
    store
        .update_entity_card(
            lu,
            &NewEntityCard {
                work_id: card.work_id,
                kind: EntityKind::Person,
                name: "陆文昭".to_string(),
                aliases: card.aliases,
                attributes: card.attributes,
                note: card.note,
            },
            "test",
        )
        .unwrap();
    let again = store.node_cast_of(first).unwrap();
    assert_eq!(again[0].name, "陆文昭", "关联认的是 id：卡改名，关联不用跟着改");

    // 另一本书看不到这一笔
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    assert!(store.outline_rows(other.id).unwrap().iter().all(|r| r.cast.is_empty()));
}

#[test]
fn a_soft_deleted_card_leaves_no_dirty_names_behind() {
    let (_dir, mut store) = fresh();
    let (work, _volume, first, _second) = book_with_two_chapters(&mut store);
    let lu = person(&mut store, work, "陆文");
    let zhang = person(&mut store, work, "老张");
    store.set_node_cast(first, &[lu, zhang], "test").unwrap();

    // 软删一张：它从这一章的人名里消失，但**另一张还在**（不是把整份名单抹了）
    store.delete_entity_card(zhang, "test").unwrap();
    let cast = store.node_cast_of(first).unwrap();
    assert_eq!(cast.len(), 1);
    assert_eq!(cast[0].entity_id, lu);

    let rows = store.outline_rows(work).unwrap();
    assert_eq!(rows.iter().find(|r| r.node_id == first).unwrap().cast.len(), 1);
}

#[test]
fn foreign_or_impossible_casts_are_refused_without_writing_anything() {
    let (_dir, mut store) = fresh();
    let (work, volume, first, _second) = book_with_two_chapters(&mut store);
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    let stranger = person(&mut store, other.id, "别家的人");

    // 别人的卡：当场拒，而且这一章一个关联都没写进去
    let err = store.set_node_cast(first, &[stranger], "test").unwrap_err();
    assert_eq!(err.code(), codes::CAST_ENTITY_FOREIGN, "{err:?}");
    assert!(store.node_cast_of(first).unwrap().is_empty(), "拒了就不许留下半份名单");

    // 卷：没有"出场人物"这回事
    let mine = person(&mut store, work, "陆文");
    let err = store.set_node_cast(volume, &[mine], "test").unwrap_err();
    assert_eq!(err.code(), codes::CAST_NODE_NO_BODY, "{err:?}");

    // 不存在的卡
    let err = store.set_node_cast(first, &[9999], "test").unwrap_err();
    assert_eq!(err.code(), codes::ENTITY_NOT_FOUND, "{err:?}");

    // 已经删掉的段
    store.soft_delete_node(first).unwrap();
    assert!(store.set_node_cast(first, &[mine], "test").is_err());
}

/// 留痕：真变了才记一笔（原样再交一次 = 幂等，不刷账本）。
#[test]
fn cast_changes_are_traced_per_change_not_per_call() {
    let (_dir, mut store) = fresh();
    let (work, _volume, first, _second) = book_with_two_chapters(&mut store);
    let lu = person(&mut store, work, "陆文");
    let zhang = person(&mut store, work, "老张");

    let before = op_log_count(&store);
    store.set_node_cast(first, &[lu], "test").unwrap();
    assert_eq!(op_log_count(&store), before + 1, "挂上一个人记一笔");

    // 原样再交一次：一个字没变 → 不记账
    store.set_node_cast(first, &[lu], "test").unwrap();
    assert_eq!(op_log_count(&store), before + 1);

    store.set_node_cast(first, &[lu, zhang], "test").unwrap();
    assert_eq!(op_log_count(&store), before + 2, "加一个人记一笔");
}
