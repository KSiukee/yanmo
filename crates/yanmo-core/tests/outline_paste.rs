//! 整片粘贴验收：**一片格子一次事务**——要么全落，要么一个字节都不落。
//!
//! 五件事（每条都对着一个真会出事的口径）：
//! 1. 一片里"有的行只落一句话、有的行只落一格"——**没提的格一个字都不许动**
//!    （照四格那条整行覆盖的路循环就会把没提的格抹成空，那是静默毁稿）；
//! 2. 一片里有一格落不了（卷 / 别人的段 / 已删的段）→ 整片当场拒，库里一个字不变；
//! 3. 认不出的栏目码当场拒；
//! 4. 一片过大当场拒（防"把整张表贴进来"）；
//! 5. 留痕按行记（一片 200 格不该把账本淹掉）。

use yanmo_core::error::codes;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{OutlineCell, Store, MAX_PASTE_CELLS};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn cell(node_id: i64, column: &str, value: &str) -> OutlineCell {
    OutlineCell { node_id, column: column.to_string(), value: value.to_string() }
}

fn op_log_count(store: &Store) -> i64 {
    store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM op_log WHERE op = 'outline_paste'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_pasted_block_fills_only_the_cells_it_carries() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();

    // 第二章已经有四格：这一片只补"结果"那一格，别的必须原样留着
    store
        .save_scene_fields(
            &yanmo_core::model::SceneFields {
                node_id: second,
                pov: "老张".to_string(),
                goal: "找到账本".to_string(),
                conflict: "衙门封了门".to_string(),
                outcome: String::new(),
            },
            "test",
        )
        .unwrap();

    let rows = store
        .save_outline_cells(
            work.id,
            &[
                // 粘进来的那一片：第 1 行两格（一句话 + 目标），第 2 行一格（结果）
                cell(first, "summary", "他第一次进城"),
                cell(first, "goal", "  认出门匾上的字  "),
                cell(second, "outcome", "账本到手"),
            ],
            "author",
        )
        .unwrap();

    let row = |id: i64| rows.iter().find(|r| r.node_id == id).unwrap().clone();
    assert_eq!(row(first).summary, "他第一次进城");
    assert_eq!(row(first).fields.goal, "认出门匾上的字", "四格入库前修剪首尾空白");
    assert_eq!(row(first).fields.pov, "", "这一片没提的格还是空的");
    assert_eq!(row(second).summary, "", "没提这一行的一句话，就不许动它");
    assert_eq!(
        (row(second).fields.pov.as_str(), row(second).fields.goal.as_str(), row(second).fields.conflict.as_str()),
        ("老张", "找到账本", "衙门封了门"),
        "★ 没提的那三格一个字都不许动（整行覆盖会把它们抹空）"
    );
    assert_eq!(row(second).fields.outcome, "账本到手");
}

#[test]
fn one_bad_cell_refuses_the_whole_block() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    let foreign = store.create_node(other.id, None, NodeKind::Chapter, "").unwrap();

    let before = store.outline_rows(work.id).unwrap();

    // 这一片里有合法的一格、也有一格粘到了卷上 → 整片拒，合法那格也不许落
    let err = store
        .save_outline_cells(
            work.id,
            &[cell(chapter, "summary", "写进去就不对"), cell(volume, "goal", "卷没有这一格")],
            "author",
        )
        .unwrap_err();
    assert_eq!(err.code(), codes::NODE_NO_FIELDS, "{err:?}");

    // 别人的段（同一个库里、另一本书）
    let err = store
        .save_outline_cells(work.id, &[cell(foreign, "summary", "串书了")], "author")
        .unwrap_err();
    assert_eq!(err.code(), codes::NODE_NO_FIELDS, "{err:?}");

    // 认不出的栏目码
    let err = store
        .save_outline_cells(work.id, &[cell(chapter, "mood", "没这一栏")], "author")
        .unwrap_err();
    assert_eq!(err.code(), codes::UNKNOWN_SCENE_FIELD, "{err:?}");
    // 章名那一列不是可填的格（界面根本不会发它，这是入库前最后一关）
    let err = store
        .save_outline_cells(work.id, &[cell(chapter, "title", "我自己起名")], "author")
        .unwrap_err();
    assert_eq!(err.code(), codes::UNKNOWN_SCENE_FIELD, "{err:?}");

    assert_eq!(store.outline_rows(work.id).unwrap(), before, "拒了就不许留下半片");
    assert_eq!(op_log_count(&store), 0, "一个字没写，也不该有留痕");
}

#[test]
fn a_too_large_block_is_refused_up_front() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "").unwrap();

    let cells: Vec<OutlineCell> =
        (0..=MAX_PASTE_CELLS).map(|_| cell(chapter, "summary", "x")).collect();
    let err = store.save_outline_cells(work.id, &cells, "author").unwrap_err();
    assert_eq!(err.code(), codes::OUTLINE_PASTE_TOO_BIG, "{err:?}");
    assert_eq!(op_log_count(&store), 0);

    // 刚好到上限：照收
    let cells: Vec<OutlineCell> = (0..MAX_PASTE_CELLS).map(|_| cell(chapter, "summary", "x")).collect();
    store.save_outline_cells(work.id, &cells, "author").unwrap();
}

/// 留痕按**行**记：一片 12 格落在 2 行上，就是 2 条。
#[test]
fn the_trace_is_one_entry_per_row_not_per_cell() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();

    let mut cells = Vec::new();
    for node in [first, second] {
        for column in ["summary", "pov", "goal", "conflict", "outcome"] {
            cells.push(cell(node, column, "x"));
        }
    }
    store.save_outline_cells(work.id, &cells, "author").unwrap();
    assert_eq!(op_log_count(&store), 2, "按行记：10 格落在 2 行 = 2 条留痕");

    // 留痕里要能看出这一行动了哪几栏（audit 时不用去猜）
    let payload: String = store
        .conn()
        .query_row("SELECT payload FROM op_log WHERE op = 'outline_paste' ORDER BY seq LIMIT 1", [], |r| r.get(0))
        .unwrap();
    for column in ["summary", "pov", "goal", "conflict", "outcome"] {
        assert!(payload.contains(column), "留痕里应写清动了哪几栏：{payload}");
    }
    assert!(payload.contains("author"), "留痕里应带上是谁触发的：{payload}");
}
