//! 字数预聚合的验收（#128）：目录树 / 卷合计 / 书架看到的是**同一套口径**的数字。
//!
//! 三件事必须成立：
//! 1. 节点上三个口径都存着，卷合计与书架合计**逐列加总**（不是只加"按词"那一列）；
//! 2. 老库（v4，只有"按词"一列）打开时**自动回填**——不靠作者去把每一章点开一次；
//! 3. 回填**只做一次**，再打开不会把算好的数动掉（也不会每次都扫全库）。

use yanmo_core::model::WorkKind;
use yanmo_core::store::Store;
use yanmo_core::text::WordCaliber;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn node_rollup_and_shelf_all_carry_the_three_calibers() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;

    let mut expected = (0, 0, 0);
    for (index, body) in ["你好，世界。", "これは Rust です"].iter().enumerate() {
        let chapter = store
            .create_node(
                work.id,
                Some(volume),
                yanmo_core::model::NodeKind::Chapter,
                &format!("第{}章", index + 1),
            )
            .unwrap();
        let stats = store.write_body(chapter, body).unwrap();
        expected.0 += stats.char_count;
        expected.1 += stats.chars_no_punct;
        expected.2 += stats.word_count;
    }

    // 逐章：三个数都挂在节点上（不必扫正文）
    let chapters: Vec<_> = store
        .list_nodes(work.id)
        .unwrap()
        .into_iter()
        .filter(|n| n.kind == yanmo_core::model::NodeKind::Chapter)
        .collect();
    let per_node: (i64, i64, i64) = chapters
        .iter()
        .fold((0, 0, 0), |acc, n| (acc.0 + n.char_count, acc.1 + n.chars_no_punct, acc.2 + n.word_count));
    assert_eq!(per_node, expected, "逐章三列加总应等于逐次写入的合计");

    // 卷合计：三列各自加总
    let rollup = store.subtree_rollup(volume).unwrap();
    assert_eq!(
        (rollup.char_count, rollup.chars_no_punct, rollup.word_count),
        expected,
        "卷合计要三列各自加总，不是只算按词"
    );

    // 书架合计：同样三列
    let shelf = store.shelf().unwrap().into_iter().find(|e| e.work.id == work.id).unwrap();
    assert_eq!(
        (shelf.char_count, shelf.chars_no_punct, shelf.word_count),
        expected,
        "书架合计也要三列各自加总"
    );

    // 三个口径各自对得上：卷合计就是"按当前口径把逐章加起来"
    for caliber in WordCaliber::ALL {
        let by_rows: i64 = chapters
            .iter()
            .map(|n| match caliber {
                WordCaliber::Chars => n.char_count,
                WordCaliber::CharsNoPunct => n.chars_no_punct,
                WordCaliber::Words => n.word_count,
            })
            .sum();
        let from_rollup = match caliber {
            WordCaliber::Chars => rollup.char_count,
            WordCaliber::CharsNoPunct => rollup.chars_no_punct,
            WordCaliber::Words => rollup.word_count,
        };
        assert_eq!(from_rollup, by_rows, "卷合计与逐章求和在 {} 口径下必须相等", caliber.as_str());
    }
}

/// 造一个"v4 时代的库"：结构到 v4 为止，节点上**只有** word_count 一列。
fn make_v4_library(path: &std::path::Path) {
    use yanmo_core::db::migrations::MIGRATIONS;
    let conn = yanmo_core::db::open(path).unwrap();
    for migration in MIGRATIONS.iter().take_while(|m| m.version <= 4) {
        for step in migration.steps {
            conn.execute(step, []).unwrap();
        }
        if let Some(prepare) = migration.prepare {
            for step in prepare(&conn).unwrap() {
                conn.execute(&step, []).unwrap();
            }
        }
    }
    conn.pragma_update(None, "user_version", 4).unwrap();
    conn.execute(
        "INSERT INTO works(id, kind, title, language, created_at, updated_at, opened_at)
         VALUES(1, 'novel', '旧稿', 'zh', 1, 1, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO nodes(id, work_id, parent_id, node_kind, title, sort_order, word_count,
             created_at, updated_at)
         VALUES(1, 1, NULL, 'volume', '第一卷', 0, 0, 1, 1)",
        [],
    )
    .unwrap();
    // 正文是假名：**旧规则把它算成 1 个词**（这就是要回填修掉的那种值）
    conn.execute(
        "INSERT INTO nodes(id, work_id, parent_id, node_kind, title, sort_order, word_count,
             created_at, updated_at)
         VALUES(2, 1, 1, 'chapter', '第1章', 0, 1, 1, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO node_contents(node_id, body, content_hash, char_count, updated_at)
         VALUES(2, 'こんにちは、世界', 'fp', 8, 1)",
        [],
    )
    .unwrap();
}

#[test]
fn an_old_library_is_backfilled_on_open_without_opening_each_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    make_v4_library(&path);

    let store = Store::open(&path).unwrap();
    let chapter = store
        .list_nodes(1)
        .unwrap()
        .into_iter()
        .find(|n| n.kind == yanmo_core::model::NodeKind::Chapter)
        .unwrap();

    assert_eq!(chapter.word_count, 7, "假名 5 逐字 + 世界 2——旧值 1 必须被回填修掉");
    assert_eq!(chapter.char_count, 8, "含标点：假名 5 + 标点 1 + 汉字 2");
    assert_eq!(chapter.chars_no_punct, 7);
    assert_eq!(
        store.subtree_rollup(1).unwrap().chars_no_punct,
        7,
        "卷合计也立刻是对的（不必打开每一章）"
    );
}

#[test]
fn the_backfill_runs_once_and_leaves_settled_numbers_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    make_v4_library(&path);

    {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.list_nodes(1).unwrap()[1].word_count, 7);
        let marker: Option<String> = store
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key = 'nodes.counts_backfilled'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert!(marker.is_some(), "回填过就要留标记（否则每次打开都扫全库）");
    }

    // 再打开一次：数字原样，不会被重算成别的（幂等）
    let store = Store::open(&path).unwrap();
    assert_eq!(store.list_nodes(1).unwrap()[1].word_count, 7);
}
