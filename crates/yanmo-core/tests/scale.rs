//! 极端规模的第一批**真数字**（"百万字不卡"的起点，不是结论）。
//!
//! 只做两件事：造得出来、读得回来、搜得到，并把耗时打出来。
//! 性能验收要拿这些数字定阈值，所以这里**不上紧断言**，只抓数量级崩坏。

use std::time::Instant;

use yanmo_core::model::{EntityKind, NewEntityCard, NewForeshadow, NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn db_bytes(store: &Store) -> i64 {
    let page_count: i64 = store.conn().query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
    let page_size: i64 = store.conn().query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
    page_count * page_size
}

/// 长篇的常态规模：1000 章 × 约 3000 字 = 约 300 万字。
#[test]
fn three_million_chars_across_a_thousand_chapters() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "三百万字").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;

    let body = "这是一句用来凑字数的正文，大约二十来个字。".repeat(150); // ≈ 3000 字
    let total_chars = body.chars().count() * 1000;

    let started = Instant::now();
    let mut ids = Vec::with_capacity(1000);
    for index in 0..1000 {
        let id = store
            .create_node(work.id, Some(volume), NodeKind::Chapter, &format!("第{index}章"))
            .unwrap();
        store.write_body(id, &body).unwrap();
        ids.push(id);
    }
    let write = started.elapsed();

    let started = Instant::now();
    let nodes = store.list_nodes(work.id).unwrap();
    let tree = started.elapsed();

    let started = Instant::now();
    let hits = store.search("凑字数", None, 50).unwrap();
    let search = started.elapsed();

    let started = Instant::now();
    let one = store.read_body(ids[500]).unwrap();
    let read = started.elapsed();

    // 「计划 vs 实际」全书对一遍：**要读全部正文做字面匹配**，是这一族里最重的一步。
    // 给它一份真实形状的计划（100 张人物卡 / 20 条还埋着的伏笔 / 每章挂两个人 + 一句话），
    // 否则量出来的只是"读一遍正文"的时间，不是这一屏真实要花的时间。
    let mut cards = Vec::new();
    for index in 0..100 {
        cards.push(
            store
                .create_entity_card(
                    &NewEntityCard {
                        work_id: work.id,
                        kind: EntityKind::Person,
                        name: format!("人名{index:03}"),
                        aliases: Vec::new(),
                        attributes: Vec::new(),
                        note: String::new(),
                    },
                    "scale",
                )
                .unwrap(),
        );
    }
    for index in 0..20 {
        store
            .create_foreshadow(
                &NewForeshadow {
                    work_id: work.id,
                    body: format!("伏笔{index:02}的那件东西"),
                    planted_node: Some(ids[index]),
                    note: String::new(),
                },
                "scale",
            )
            .unwrap();
    }
    for (index, id) in ids.iter().enumerate() {
        store.set_node_summary(*id, "一句话章纲").unwrap();
        store
            .set_node_cast(*id, &[cards[index % 100], cards[(index + 7) % 100]], "scale")
            .unwrap();
    }
    let started = Instant::now();
    let actual = store.outline_actuals(work.id).unwrap();
    let scan = started.elapsed();

    println!(
        "[规模] 1000 章 / 共约 {total_chars} 字：写入 {write:?}｜全树 {tree:?}（{} 节点）｜\
         检索 {search:?}（命中 {}）｜读一章 {read:?}｜计划vs实际 {scan:?}（{} 章）｜库 {:.1} MB",
        nodes.len(),
        hits.len(),
        actual.chapters.len(),
        db_bytes(&store) as f64 / 1024.0 / 1024.0
    );

    assert_eq!(nodes.len(), 1001, "一卷 + 1000 章");
    assert!(!hits.is_empty(), "三百万字里必须搜得到");
    assert_eq!(one, body);
    assert!(write.as_secs() < 60, "写入耗时数量级异常：{write:?}");
    assert!(search.as_secs() < 10, "检索耗时数量级异常：{search:?}");
    assert_eq!(actual.truncated, 0, "1000 章还没到安全阀");
    assert!(scan.as_secs() < 30, "全书对一遍耗时数量级异常：{scan:?}");
}

/// 病态场景：**单章** 300 万字（正常写作不会这样，但粘贴/导入可能造出来）。
#[test]
#[ignore = "病态场景：单章 300 万字，按需运行（--ignored）"]
fn single_chapter_with_three_million_chars() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "单章三百万字").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;

    let body = "病态单章的正文内容，用来量一量最坏情况。".repeat(150_000); // ≈ 300 万字
    let chars = body.chars().count();

    let started = Instant::now();
    let stats = store.write_body(node, &body).unwrap();
    let write = started.elapsed();

    let started = Instant::now();
    let back = store.read_body(node).unwrap();
    let read = started.elapsed();

    let started = Instant::now();
    let hits = store.search("最坏情况", None, 10).unwrap();
    let search = started.elapsed();

    println!(
        "[病态] 单章 {chars} 字（{} 字口径）：写入 {write:?}｜读回 {read:?}（{} 字）｜\
         检索 {search:?}（命中 {}）｜库 {:.1} MB",
        stats.word_count,
        back.chars().count(),
        hits.len(),
        db_bytes(&store) as f64 / 1024.0 / 1024.0
    );

    assert_eq!(back, body);
    assert_eq!(hits.len(), 1);
}
