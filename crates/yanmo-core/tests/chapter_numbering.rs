//! 章节编号的两种数法：**跨卷延续（默认）** 与 **每卷从头数**。
//!
//! 这是"号 = 位置的函数"那条铁律的延伸：标题里存的还是模板，改设置只改**渲染时怎么数**，
//! 所以正文与标题一个字都不动、切一下立刻全见效（目录、导出、编译看到的是同一份结果）。
//!
//! 验收集中在一件事上：**三条读路径必须给出同一个号**——
//! 整树（`list_nodes`）、单层懒加载（`children_of`，目录树展开用的就是它）、
//! 单节点（`rendered_title`）；再加导出与"作者手写的 `{$N_RESET}` 仍然说话"。

use yanmo_core::error::codes;
use yanmo_core::model::{ChapterNumbering, NodeKind, WorkKind};
use yanmo_core::store::{Appearance, ExportFormat, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本两卷的书：卷一 3 章（第 2 章是"序章"这种不占号的自起名），卷二 2 章。
fn two_volume_book() -> (tempfile::TempDir, Store, i64, Vec<i64>, Vec<i64>) {
    let (dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume_one = store.list_nodes(work.id).unwrap()[0].id;
    let volume_two = store.create_node(work.id, None, NodeKind::Volume, "归途").unwrap();
    let mut chapters = Vec::new();
    for (parent, title) in [
        (volume_one, ""),
        (volume_one, "序章"), // 没有计数宏：不占号、也不推号
        (volume_one, ""),
        (volume_two, ""),
        (volume_two, ""),
    ] {
        let id = store.create_node(work.id, Some(parent), NodeKind::Chapter, title).unwrap();
        store.write_body(id, "正文。").unwrap(); // 导出要按阅读顺序出正文，空章不占一行
        chapters.push(id);
    }
    (dir, store, work.id, vec![volume_one, volume_two], chapters)
}

fn rendered(store: &Store, work_id: i64) -> Vec<(i64, String)> {
    store
        .list_nodes(work_id)
        .unwrap()
        .into_iter()
        .map(|node| (node.id, node.title_rendered))
        .collect()
}

fn look(store: &Store, work_id: i64, id: i64) -> String {
    rendered(store, work_id)
        .into_iter()
        .find(|(node_id, _)| *node_id == id)
        .expect("节点在整树里")
        .1
}

#[test]
fn by_default_chapter_numbers_continue_across_volumes() {
    let (_dir, store, work_id, volumes, chapters) = two_volume_book();
    assert_eq!(
        store.appearance(Some(work_id)).unwrap().chapter_numbering,
        ChapterNumbering::Continue,
        "默认就是跨卷延续（没设过也不写库）"
    );

    assert_eq!(look(&store, work_id, chapters[0]), "第1章");
    assert_eq!(look(&store, work_id, chapters[1]), "序章", "自起名不占号也不推号");
    assert_eq!(look(&store, work_id, chapters[2]), "第2章");
    // ★ 第二卷接着数：不是"第1章"
    assert_eq!(look(&store, work_id, chapters[3]), "第3章");
    assert_eq!(look(&store, work_id, chapters[4]), "第4章");
    // 卷自己仍是按位置排：第1卷 / 第2卷（作者给第二卷起了名也不影响它占号）
    assert_eq!(look(&store, work_id, volumes[0]), "第1卷");
    assert_eq!(look(&store, work_id, volumes[1]), "归途");
}

#[test]
fn every_read_path_gives_the_same_number() {
    let (_dir, store, work_id, volumes, chapters) = two_volume_book();
    // 懒加载一次只拉一层——目录树展开第二卷走的正是这条
    let second = store.children_of(work_id, Some(volumes[1])).unwrap();
    let second_titles: Vec<String> =
        second.iter().map(|node| node.title_rendered.clone()).collect();
    assert_eq!(second_titles, ["第3章", "第4章"], "单层也要接上前面数过的号");
    assert_eq!(store.rendered_title(chapters[4]).unwrap(), "第4章", "单节点同一条来路");
    assert_eq!(store.rendered_title(volumes[1]).unwrap(), "归途");
}

#[test]
fn per_volume_mode_can_be_turned_back_on_and_is_remembered_per_work() {
    let (_dir, mut store, work_id, _volumes, chapters) = two_volume_book();
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    let other_volume = store.list_nodes(other.id).unwrap()[0].id;
    let other_chapter =
        store.create_node(other.id, Some(other_volume), NodeKind::Chapter, "").unwrap();

    // 只把这一本改成"每卷从头数"（全局默认不动）
    store
        .set_appearance(
            Some(work_id),
            &Appearance { chapter_numbering: Some("per_volume".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(look(&store, work_id, chapters[3]), "第1章", "第二卷从头数");
    assert_eq!(look(&store, work_id, chapters[4]), "第2章");
    assert_eq!(look(&store, work_id, chapters[2]), "第2章", "第一卷不受影响");
    assert_eq!(
        store.appearance(Some(other.id)).unwrap().chapter_numbering,
        ChapterNumbering::Continue,
        "另一本仍跟着全局默认"
    );
    assert_eq!(store.rendered_title(other_chapter).unwrap(), "第1章");

    // 全局改成"每卷从头数"：没单独设过的书跟着变
    store
        .set_appearance(
            None,
            &Appearance { chapter_numbering: Some("per_volume".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(store.appearance(Some(other.id)).unwrap().chapter_numbering, ChapterNumbering::PerVolume);
    // 而单独设过的那一本，显式设成"跟随默认"才回去
    store
        .set_appearance(
            Some(work_id),
            &Appearance { chapter_numbering: Some("auto".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(store.appearance(Some(work_id)).unwrap().chapter_numbering, ChapterNumbering::PerVolume);

    // 改设置**不动标题一个字**：库里存的还是模板
    assert_eq!(store.node_title(chapters[3]).unwrap(), "第{$N}章");
}

#[test]
fn an_explicit_reset_still_speaks() {
    let (_dir, mut store, work_id, volumes, chapters) = two_volume_book();
    // 第二卷第一章写 `{$N_RESET:101}`：显式指令到哪儿都说话，后面的章接着它往下数
    store.rename_node(chapters[3], "第{$N}章{$N_RESET:101}").unwrap();
    assert_eq!(look(&store, work_id, chapters[2]), "第2章", "第一卷照旧");
    assert_eq!(look(&store, work_id, chapters[3]), "第101章");
    assert_eq!(look(&store, work_id, chapters[4]), "第102章");
    // 走懒加载那条路也是同一个结果
    let second = store.children_of(work_id, Some(volumes[1])).unwrap();
    assert_eq!(
        second.iter().map(|node| node.title_rendered.clone()).collect::<Vec<_>>(),
        ["第101章", "第102章"]
    );
}

#[test]
fn exports_follow_the_same_numbers() {
    let (_dir, store, work_id, _volumes, chapters) = two_volume_book();
    // 分章 txt 的**文件名**就是渲染后的标题（号在导出里也必须接着数）
    let paths: Vec<String> = store
        .render_work(work_id, ExportFormat::Text)
        .unwrap()
        .into_iter()
        .map(|file| file.relative_path)
        .collect();
    assert!(
        paths.iter().any(|path| path.contains("第3章")),
        "第二卷第一章在导出里也得是第 3 章：{paths:?}"
    );
    assert!(paths.iter().any(|path| path.contains("第4章")), "{paths:?}");
    assert!(paths.iter().any(|path| path.contains("序章")), "{paths:?}");
    assert_eq!(look(&store, work_id, chapters[3]), "第3章");
}

#[test]
fn an_unknown_numbering_code_is_rejected_not_guessed() {
    let (_dir, mut store, work_id, _volumes, _chapters) = two_volume_book();
    let error = store
        .set_appearance(
            Some(work_id),
            &Appearance { chapter_numbering: Some("rotate".into()), ..Default::default() },
        )
        .unwrap_err();
    assert_eq!(error.code(), codes::UNKNOWN_CHAPTER_NUMBERING);
    // 库里那条坏记录（旧版本写的）当没设过 → 回默认
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, 0)",
            rusqlite::params![format!("work.{work_id}.appearance"), r#"{"chapter_numbering":"spin"}"#],
        )
        .unwrap();
    assert_eq!(
        store.appearance(Some(work_id)).unwrap().chapter_numbering,
        ChapterNumbering::Continue
    );
}

#[test]
fn an_unclosed_macro_does_not_swallow_the_macros_after_it() {
    // 2026-09-15 代码质量评审：轻微 11——作者手写坏一个 `{$`（没闭合）时，老实现从它一直扫到
    // 下一个 `}`，把中间那个**合法宏**也当成坏宏吞掉了，表现为"后面的号不渲染"。
    // 现在跳过这一个继续扫。
    assert_eq!(
        yanmo_core::numbering::render("{$ 手写坏了 第{$N}章", 7),
        "{$ 手写坏了 第7章",
        "未闭合的宏只该影响它自己"
    );
    // 末尾那个未闭合的宏原样留着（作者看得见），前面的合法宏照常渲染
    assert_eq!(yanmo_core::numbering::render("第{$N}章 {$", 3), "第3章 {$");
    assert_eq!(yanmo_core::numbering::render("第{$N}章", 12), "第12章");
}
