//! 短篇集的**写入口**验收：加 / 拿掉 / 重排 / 定目标。
//!
//! 一条总口径贯穿全篇：**集只是引用，从不碰正文**——加、拿掉、重排、软删集，
//! 那一篇稿子本身（正文、字数、稿件台的状态与投稿史）一个字节都不该变。

use yanmo_core::error_codes::codes;
use yanmo_core::model::WorkKind;
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 建一篇有字数的短篇。
fn article(store: &mut Store, title: &str, body: &str) -> i64 {
    let work = store.create_work(WorkKind::Article, title).unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(root, body).unwrap();
    work.id
}

fn collection(store: &mut Store, title: &str) -> i64 {
    store.create_work(WorkKind::Collection, title).unwrap().id
}

/// 集里的篇目标题（按集内顺序）——顺序是这一份验收盯得最紧的东西。
fn titles(store: &Store, collection_id: i64) -> Vec<String> {
    store
        .collection_view(collection_id)
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.title)
        .collect()
}

/// 还没归集的散篇（标题）。
fn uncollected(store: &Store) -> Vec<String> {
    store
        .uncollected_articles()
        .unwrap()
        .into_iter()
        .map(|entry| entry.title)
        .collect()
}

#[test]
fn adding_an_article_appends_it_and_takes_it_out_of_the_loose_pile() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "年度自选");
    let first = article(&mut store, "第一篇", "一二三");
    let second = article(&mut store, "第二篇", "四五六");

    assert!(store.add_to_collection(anthology, first).unwrap());
    assert!(store.add_to_collection(anthology, second).unwrap());
    assert_eq!(titles(&store, anthology), vec!["第一篇", "第二篇"], "追加在末尾");
    assert!(uncollected(&store).is_empty(), "归集之后都不再是散篇");
}

#[test]
fn the_same_piece_can_sit_in_two_collections_and_adding_twice_is_a_no_op() {
    let (_dir, mut store) = fresh();
    let year = collection(&mut store, "年度自选");
    let sci = collection(&mut store, "科幻集");
    let piece = article(&mut store, "同一篇", "正文");

    assert!(store.add_to_collection(year, piece).unwrap());
    assert!(store.add_to_collection(sci, piece).unwrap());
    assert_eq!(store.collections_of(piece).unwrap(), vec![year, sci]);
    assert!(!store.add_to_collection(year, piece).unwrap(), "加两次是空操作");
    assert_eq!(titles(&store, year).len(), 1, "同一集里不会出现两次");
    assert_eq!(store.collections_of(piece).unwrap().len(), 2, "两个集都记着它");
}

#[test]
fn removing_only_drops_the_reference() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let piece = article(&mut store, "一篇", "正文正文");
    store.add_to_collection(anthology, piece).unwrap();
    let before = store.get_work(piece).unwrap();

    assert!(store.remove_from_collection(anthology, piece).unwrap());
    assert!(titles(&store, anthology).is_empty());
    assert_eq!(uncollected(&store), vec!["一篇"], "拿掉之后又变回散篇");
    assert!(!store.remove_from_collection(anthology, piece).unwrap(), "本来就不在：空操作");
    assert_eq!(store.get_work(piece).unwrap().title, before.title, "正文与作品本身没动");
    assert_eq!(store.shelf().unwrap().len(), 2, "作品都还在书架上");
}

#[test]
fn only_articles_go_in_and_only_collections_take_them() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap().id;
    let piece = article(&mut store, "一篇", "正文");

    let member = store.add_to_collection(anthology, novel).unwrap_err();
    assert_eq!(member.code(), codes::COLLECTION_MEMBER_NOT_ARTICLE);
    let host = store.add_to_collection(piece, piece).unwrap_err();
    assert_eq!(host.code(), codes::COLLECTION_NOT_A_COLLECTION);
    // 被拒绝时不该留下半条引用
    assert!(store.collection_view(anthology).unwrap().entries.is_empty());
}

#[test]
fn reordering_follows_the_given_order_and_refuses_a_stale_list() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let a = article(&mut store, "甲", "正文");
    let b = article(&mut store, "乙", "正文");
    let c = article(&mut store, "丙", "正文");
    for id in [a, b, c] {
        store.add_to_collection(anthology, id).unwrap();
    }

    store.reorder_collection(anthology, &[c, a, b]).unwrap();
    assert_eq!(titles(&store, anthology), vec!["丙", "甲", "乙"]);

    // 少一篇 / 多一篇都拒绝，且**顺序一点没动**
    let stale = store.reorder_collection(anthology, &[c, a]).unwrap_err();
    assert_eq!(stale.code(), codes::COLLECTION_REORDER_MISMATCH);
    let extra = store.reorder_collection(anthology, &[c, a, b, a]).unwrap_err();
    assert_eq!(extra.code(), codes::COLLECTION_REORDER_MISMATCH);
    assert_eq!(titles(&store, anthology), vec!["丙", "甲", "乙"], "拒绝时不该改顺序");
}

#[test]
fn the_collection_target_can_be_set_changed_and_cleared() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let view = store.collection_view(anthology).unwrap();
    assert_eq!(view.target_pieces, None, "一开始没设过");
    assert_eq!(view.target_words, None);

    store.set_collection_target(anthology, Some(8), Some(60_000)).unwrap();
    let view = store.collection_view(anthology).unwrap();
    assert_eq!(view.target_pieces, Some(8));
    assert_eq!(view.target_words, Some(60_000));

    store.set_collection_target(anthology, Some(0), None).unwrap();
    let view = store.collection_view(anthology).unwrap();
    assert_eq!(view.target_pieces, None, "≤0 当清掉");
    assert_eq!(view.target_words, None, "没给的那一项也是清掉");
}
