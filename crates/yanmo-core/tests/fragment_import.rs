//! 外部文本导入的验收：来源、去重、整批可撤、不碰正文。

use yanmo_core::model::{FragmentKind, NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn import_is_sourced_deduped_and_undoable_without_touching_the_manuscript() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "第一章")
        .unwrap();
    store.write_body(chapter, "正文原样。").unwrap();
    let before = store.read_body(chapter).unwrap();

    let items = yanmo_core::store::split_import_text(
        "灵感一\n\n灵感二\n\n灵感一",
        "blank",
    )
    .unwrap();
    assert_eq!(items, vec!["灵感一", "灵感二", "灵感一"]);

    let outcome = store
        .import_fragments(work.id, FragmentKind::Idea, "备忘录.md", "pasted", &items)
        .unwrap();
    assert_eq!(outcome.imported, 2, "重复的一条不该再存");
    assert_eq!(outcome.skipped, 1);
    assert!(!outcome.batch_id.is_empty());

    let fragments = store.fragments(work.id, &[FragmentKind::Idea], 20).unwrap();
    assert_eq!(fragments.len(), 2);
    assert!(fragments.iter().all(|f| f.source == "pasted"), "来源必须如实写 pasted");

    let batches = store.import_batches(work.id, 10).unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].label, "备忘录.md");
    assert_eq!(batches[0].count, 2);
    assert!(!batches[0].undone);

    let removed = store.undo_import(&outcome.batch_id).unwrap();
    assert_eq!(removed, 2);
    assert_eq!(store.fragments(work.id, &[FragmentKind::Idea], 20).unwrap().len(), 0);
    assert!(store.import_batches(work.id, 10).unwrap()[0].undone);

    assert_eq!(store.read_body(chapter).unwrap(), before, "导入/撤销都不许碰正文");
}

#[test]
fn split_modes_are_visible_and_do_not_rewrite_the_text() {
    let (_dir, _store) = fresh();
    let text = "第一段，有标点。\n\n第二段\n还是第二段\r\n";
    let blank = yanmo_core::store::split_import_text(text, "blank").unwrap();
    assert_eq!(blank, vec!["第一段，有标点。", "第二段\n还是第二段"]);
    let line = yanmo_core::store::split_import_text(text, "line").unwrap();
    assert_eq!(line.len(), 3);
    let whole = yanmo_core::store::split_import_text(text, "whole").unwrap();
    assert_eq!(whole.len(), 1);
    assert!(whole[0].contains("第二段\n还是第二段"));
}
