//! 集**读数**的验收：篇目汇总、软删的篇与软删的集、还没归集的散篇。
//!
//! 两条边界最要紧：**软删的篇不进集视图**（但引用留着，捞回来还在原位）、
//! **软删的集不算集**（它引用的篇目重新回到"未归集的散篇"里）。

use yanmo_core::model::WorkKind;
use yanmo_core::store::Store;

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

fn collection(store: &mut Store, title: &str) -> i64 {
    store.create_work(WorkKind::Collection, title).unwrap().id
}

fn uncollected(store: &Store) -> Vec<String> {
    store
        .uncollected_articles()
        .unwrap()
        .into_iter()
        .map(|entry| entry.title)
        .collect()
}

#[test]
fn the_view_carries_the_order_and_the_rollup() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let a = article(&mut store, "甲", "一二三");
    let b = article(&mut store, "乙", "四五六七八");
    store.add_to_collection(anthology, a).unwrap();
    store.add_to_collection(anthology, b).unwrap();

    let view = store.collection_view(anthology).unwrap();
    assert_eq!(view.collection.title, "自选");
    assert_eq!(view.pieces(), 2);
    assert_eq!(view.chars(), 8, "逐字含标点：3 + 5");
    assert_eq!(view.entries[0].char_count, 3);
    assert_eq!(view.entries[0].id, a);
    assert_eq!(view.entries[1].id, b);
    assert_eq!(view.entries[0].status, yanmo_core::model::ManuscriptState::Draft);
}

#[test]
fn a_trashed_piece_stays_out_of_the_view_but_keeps_its_slot() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let kept = article(&mut store, "留着", "正文");
    let trashed = article(&mut store, "进了回收站", "正文");
    store.add_to_collection(anthology, kept).unwrap();
    store.add_to_collection(anthology, trashed).unwrap();

    store.soft_delete_work(trashed).unwrap();
    let view = store.collection_view(anthology).unwrap();
    assert_eq!(view.pieces(), 1, "回收站里的那篇不该还挂在集里");
    assert_eq!(view.entries[0].id, kept);
    assert!(uncollected(&store).is_empty(), "归集的不算散篇，回收站里的也不算");

    // 捞回来还在集里（引用没删，顺序也没重排）
    store.restore_work(trashed).unwrap();
    let view = store.collection_view(anthology).unwrap();
    assert_eq!(view.entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![kept, trashed]);
}

#[test]
fn a_trashed_collection_frees_its_articles_back_to_the_loose_pile() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let piece = article(&mut store, "一篇", "正文");
    store.add_to_collection(anthology, piece).unwrap();
    assert!(uncollected(&store).is_empty());

    store.soft_delete_work(anthology).unwrap();
    assert_eq!(uncollected(&store), vec!["一篇"], "集没了，篇就散着");
    assert!(store.collections_of(piece).unwrap().is_empty(), "不算属于任何活着的集");

    store.restore_work(anthology).unwrap();
    assert!(uncollected(&store).is_empty(), "捞回集之后篇又归位");
}

#[test]
fn loose_articles_are_the_ones_in_no_alive_collection() {
    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "自选");
    let collected = article(&mut store, "已归集", "正文");
    let loose = article(&mut store, "还散着", "正文");
    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap().id;
    store.add_to_collection(anthology, collected).unwrap();

    let loose_now = uncollected(&store);
    assert_eq!(loose_now, vec!["还散着"], "长篇不进这张单子，已归集的也不进");
    assert!(!loose_now.contains(&"长夜".to_string()));
    assert!(store.collections_of(loose).unwrap().is_empty());
    assert_eq!(store.collections_of(collected).unwrap(), vec![anthology]);
    assert!(store.collections_of(novel).unwrap().is_empty());
}

/// 迁移 v18 幂等：已经在 v18 上的库把版本退回 17 再打开，不炸、引用一条不少。
#[test]
fn migration_eighteen_is_idempotent_on_an_existing_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let (anthology, piece) = {
        let mut store = Store::open(&path).unwrap();
        let anthology = collection(&mut store, "自选");
        let piece = article(&mut store, "一篇", "正文");
        store.add_to_collection(anthology, piece).unwrap();
        store.conn().pragma_update(None, "user_version", 17).unwrap();
        (anthology, piece)
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
    assert_eq!(store.collection_view(anthology).unwrap().entries[0].id, piece);
}

/// 集导出：**按集内顺序**合成一份成品，自带目录；空集也给一句话。
#[test]
fn exporting_a_collection_keeps_the_order_and_adds_a_contents_page() {
    use yanmo_core::compile::{compile_collection, CollectionFormat};

    let (_dir, mut store) = fresh();
    let anthology = collection(&mut store, "年度自选");
    let a = article(&mut store, "甲篇", "甲的第一段。\n\n甲的第二段。");
    let b = article(&mut store, "乙篇", "乙只有一段。");
    store.add_to_collection(anthology, a).unwrap();
    store.add_to_collection(anthology, b).unwrap();
    store.reorder_collection(anthology, &[b, a]).unwrap();

    let docx = compile_collection(&store, anthology, CollectionFormat::Docx).unwrap();
    assert_eq!(docx.len(), 1);
    assert_eq!(docx[0].relative_path, "collection/年度自选.docx");
    // docx 是压缩包：正文里看不到明文，这里只钉"它真的是个能打开的 ZIP"
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(docx[0].content.clone())).unwrap();
    let mut document = String::new();
    {
        use std::io::Read;
        zip.by_name("word/document.xml").unwrap().read_to_string(&mut document).unwrap();
    }
    assert!(document.contains("目录"), "集导出自带目录");
    assert!(document.contains("1. 乙篇") && document.contains("2. 甲篇"), "目录按集内顺序编号：{document}");
    let first = document.find("乙只有一段。").unwrap();
    let second = document.find("甲的第一段。").unwrap();
    assert!(first < second, "正文也按集内顺序排");
    assert!(document.contains("w:br w:type=\"page\""), "每篇各起一页");

    let txt = compile_collection(&store, anthology, CollectionFormat::MergedTxt).unwrap();
    let plain = String::from_utf8(txt[0].content.clone()).unwrap();
    assert!(plain.contains("目录"));
    assert!(plain.find("1. 乙篇").unwrap() < plain.find("2. 甲篇").unwrap());
    assert!(plain.find("乙只有一段。").unwrap() < plain.find("甲的第一段。").unwrap());
    assert_eq!(txt[0].relative_path, "collection_txt/年度自选.txt");

    // 渲染两遍逐字节一样（与别的预设同一条纪律）
    let again = compile_collection(&store, anthology, CollectionFormat::MergedTxt).unwrap();
    assert_eq!(txt, again);
}

#[test]
fn an_empty_collection_still_gets_a_file_with_a_sentence() {
    use yanmo_core::compile::{compile_collection, CollectionFormat};

    let (_dir, mut store) = fresh();
    let empty = collection(&mut store, "还没编的集");
    let txt = compile_collection(&store, empty, CollectionFormat::MergedTxt).unwrap();
    let plain = String::from_utf8(txt[0].content.clone()).unwrap();
    assert!(plain.contains("还没编的集"));
    assert!(plain.contains("目录"));

    // 传错格式：明确报错，不悄悄给一份别的
    assert!(CollectionFormat::parse("epub").is_err());
}
