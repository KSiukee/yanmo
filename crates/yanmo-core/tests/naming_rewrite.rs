//! 「整本书一起换编号写法」的验收：**只换编号，不动章名；没有编号的一个都不碰**。
//!
//! 场景就是作者真会遇到的：先把书写成 `第1章` 的样子，写了三十章之后觉得 `第一章` 更有味道——
//! 改设置只影响以后新建的（那是铁律），要把已有的换过去，就得走这个**先预览、再执行**的动作。

use yanmo_core::error_codes::codes;
use yanmo_core::model::{NodeKind, NamingStyle, WorkKind};
use yanmo_core::store::{Appearance, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本"写了三十章"的长篇：`第{$N}章 灯` 那种写法，另有两条不带编号的。
fn thirty_chapter_book(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "序章")
        .unwrap();
    for at in 0..30 {
        let title = format!("第{{$N}}章 {}", if at % 2 == 0 { "灯" } else { "门" });
        let id = store.create_node(work.id, Some(volume), NodeKind::Chapter, &title).unwrap();
        store.write_body(id, "正文。").unwrap();
    }
    store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "番外·夜谈")
        .unwrap();
    (work.id, volume)
}

#[test]
fn switching_the_style_rewrites_every_numbered_chapter_and_leaves_the_rest_alone() {
    let (_dir, mut store) = fresh();
    let (work_id, volume) = thirty_chapter_book(&mut store);

    // 作者改主意：这本书改成中文数字
    store
        .set_appearance(
            Some(work_id),
            &Appearance { naming: Some("chinese".into()), ..Default::default() },
        )
        .unwrap();

    // ① 先看清单：**三十章都列出来，两条不带编号的不在清单里**
    let preview = store.preview_naming_rewrite(work_id).unwrap();
    assert_eq!(preview.len(), 30, "只有带编号的那三十章要改");
    assert_eq!(preview[0].before, "第{$N}章 灯");
    assert_eq!(preview[0].after, "第{$N_ZH}章 灯", "章名一个字不动，只换编号写法");
    assert_eq!(preview[1].before, "第{$N}章 门");
    assert_eq!(preview[1].after, "第{$N_ZH}章 门");

    // ② 执行（照预览那份清单）
    let changed = store.apply_naming_rewrite(work_id, &preview).unwrap();
    assert_eq!(changed, 30);

    // ③ 显示出来就是中文数字，且仍旧按位置 1..30
    let rows: Vec<(String, String)> = store
        .list_nodes(work_id)
        .unwrap()
        .into_iter()
        .filter(|node| node.parent_id == Some(volume))
        .map(|node| (node.title, node.title_rendered))
        .collect();
    assert_eq!(rows[0].1, "序章", "不带编号的，一个字都没动");
    assert_eq!(rows[1].1, "第一章 灯");
    assert_eq!(rows[2].1, "第二章 门");
    assert_eq!(rows[30].1, "第三十章 门", "第 30 章（下标 29 → 门）");
    assert_eq!(rows[31].1, "番外·夜谈", "不带编号的，一个字都没动");
    assert_eq!(rows[31].0, "番外·夜谈", "库里存的也还是原样");

    // ④ 再换回去也应当干净（同一批，反向一档）
    store
        .set_appearance(
            Some(work_id),
            &Appearance { naming: Some("arabic".into()), ..Default::default() },
        )
        .unwrap();
    let back = store.preview_naming_rewrite(work_id).unwrap();
    assert_eq!(back.len(), 30);
    store.apply_naming_rewrite(work_id, &back).unwrap();
    let rendered: Vec<String> = store
        .list_nodes(work_id)
        .unwrap()
        .into_iter()
        .filter(|node| node.parent_id == Some(volume))
        .map(|node| node.title_rendered)
        .collect();
    assert_eq!(rendered[1], "第1章 灯");
    assert_eq!(rendered[30], "第30章 门");
}

#[test]
fn hand_written_numbers_are_adopted_and_plain_names_are_never_touched() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    for title in ["第1章 灯", "第 2 章 门", "序章", "番外"] {
        store.create_node(work.id, Some(volume), NodeKind::Chapter, title).unwrap();
    }

    let preview = store.preview_naming_rewrite(work.id).unwrap();
    assert_eq!(preview.len(), 2, "手写的阿拉伯数字认出来了，序章与番外不碰");
    assert_eq!(preview[0].after, "第{$N}章 灯");
    assert_eq!(preview[1].after, "第{$N}章 门");

    store.apply_naming_rewrite(work.id, &preview).unwrap();
    let rendered: Vec<String> = store
        .list_nodes(work.id)
        .unwrap()
        .into_iter()
        .filter(|node| node.parent_id == Some(volume))
        .map(|node| node.title_rendered)
        .collect();
    // 换完之后按位置重排：序章不占号，所以两章还是第1、第2章
    assert_eq!(rendered, ["第1章 灯", "第2章 门", "序章", "番外"]);
}

#[test]
fn nothing_to_do_when_the_style_is_already_right_or_means_no_numbering() {
    let (_dir, mut store) = fresh();
    let (work_id, _) = thirty_chapter_book(&mut store);

    // 已经是阿拉伯数字：清单是空的（界面据此说"不用换"）
    assert!(store.preview_naming_rewrite(work_id).unwrap().is_empty());

    // 选「不编号」：**不动已有章的名字**——那是"以后新建的"，抹掉已有的名字不是这个动作该干的事
    store
        .set_appearance(
            Some(work_id),
            &Appearance { naming: Some("none".into()), ..Default::default() },
        )
        .unwrap();
    assert!(store.preview_naming_rewrite(work_id).unwrap().is_empty());
    assert_eq!(store.naming_style(work_id).unwrap(), NamingStyle::NoNumber);
}

#[test]
fn a_rewrite_that_fails_halfway_leaves_the_book_untouched() {
    // 2026-09-15 代码质量评审：严重 4。整批换写法必须"**要么全成、要么全不成**"：
    // 这里让清单里**第二条**指向一个已经不在了的节点（作者刚把它删了），第一条完全合法——
    // 第一条也一个字都不许改。老写法是逐条各自提交，会留下"第一张中文数字、其余还是阿拉伯数字"。
    let (_dir, mut store) = fresh();
    let (work_id, _) = thirty_chapter_book(&mut store);
    store
        .set_appearance(
            Some(work_id),
            &Appearance { naming: Some("chinese".into()), ..Default::default() },
        )
        .unwrap();

    let preview = store.preview_naming_rewrite(work_id).unwrap();
    assert!(preview.len() >= 2);
    store.soft_delete_node(preview[1].node_id).unwrap(); // 作者刚删了第二章

    let error = store
        .apply_naming_rewrite(work_id, &preview)
        .expect_err("清单里有一个已经不在了的节点，整批必须拒绝");
    assert_eq!(error.code(), codes::NODE_GONE, "{error}");

    // 排在坏的那条**前面**的那一条必须原封不动——这就是这条修复的全部意义
    let stored: Vec<(i64, String)> = store
        .list_nodes(work_id)
        .unwrap()
        .into_iter()
        .map(|node| (node.id, node.title))
        .collect();
    let first = stored
        .iter()
        .find(|(id, _)| *id == preview[0].node_id)
        .expect("第一章还在")
        .1
        .clone();
    assert_eq!(first, preview[0].before, "半途失败不许留下半本");
}
