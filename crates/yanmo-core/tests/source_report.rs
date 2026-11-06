//! 来源清单的验收：**给编辑看的那一页**——只有字数、没有正文、口径与状态栏一致。

use yanmo_core::model::{EditOrigin, NodeKind, WorkKind, WriteOrigin};
use yanmo_core::store::{Appearance, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn the_report_lists_sources_per_chapter_and_never_leaks_the_body() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜渡口").unwrap();
    store
        .set_appearance(
            Some(work.id),
            &Appearance { naming: Some("chinese".to_string()), ..Default::default() },
        )
        .unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "第{$N}章 渡口无灯")
        .unwrap();

    // 先打出第一句，再整段粘进来一句：两档来源都要出现在清单里
    store
        .write_body_counted(chapter, "入冬以后。", 480, WriteOrigin::of(EditOrigin::Typed))
        .unwrap();
    store
        .write_body_counted(
            chapter,
            "入冬以后。船靠岸，女人下船。",
            480,
            WriteOrigin::of(EditOrigin::Pasted),
        )
        .unwrap();

    let text = store.source_report_text(work.id).unwrap();
    assert!(text.contains("《长夜渡口》"), "清单要写书名：{text}");
    assert!(text.contains("第1章 渡口无灯"), "章节标题要渲染过：{text}");
    assert!(!text.contains("{$"), "不能露内部模板：{text}");
    assert!(text.contains("键入 5"), "键入的字数要列出来：{text}");
    assert!(text.contains("粘贴"), "粘贴的字数要列出来：{text}");
    assert!(text.contains("合计："), "末尾要有合计：{text}");
    assert!(
        !text.contains("船靠岸"),
        "清单只给字数，不能把正文句带出去：{text}"
    );
}
