//! 极深目录树：**写得进去的树，必须读得出来**。
//!
//! # 来龙去脉（2026-09-14 由演练台的「极深目录树」那一场发现）
//!
//! `create_node` 没有深度守门，而读路径有层数上限——于是能造出「写得进、读不了」的树：
//!
//! - `node_ancestors` 到那一层直接报错（错误码还是「疑似成环」，文不对题）→
//!   界面「打开这一章沿祖先链展开」当场失败，那一章等于打不开；
//! - `subtree_rollup` 更糟，它**静默截断**：卷行字数悄悄变少——正是本项目最不容忍的
//!   「说 ok 但数字不对」。
//!
//! # 定案
//!
//! **入口守门**：新建与移动都在写之前拒绝，能写进去的树永远在能读出来的深度以内；
//! 读路径的上限只作坏数据的兜底，而且**碰到就报错，绝不少算**。
//!
//! 同一次演练还量出第二件事：那个上限（当时是 512）**比代码实际走得动的深度还大**——
//! 调试构建在 **384 层**导出成 json 就 `has overflowed its stack`（发布构建 512 层能过），
//! 也就是"合法数据"里含着能把进程搞崩的一档。上限因此收到 **64**（见 `store::MAX_TREE_DEPTH`，
//! 那里写了实测数据与理由）：比崩点留 ≥4 倍余量，且远宽于任何真实书的大纲（2~4 层）。
//!
//! 实测证据（修复前的开发构建，命令行连续 `new-node --parent` 600 次）：
//! 600 层全部建成、正文写读正常、导出 600 个文件——**核心一次都没拦**。

use yanmo_core::db;
use yanmo_core::error_codes::codes;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

/// 试探上限时最多往下建多少层：**上限本身不写死**（那是核心的事），
/// 但要比它高出一截，否则"没拦住"会伪装成"到头了"。
const TRY_LEVELS: usize = 600;
/// 深度预算的**下限**：从成稿导入那条路承诺"32 层能进能出"（见 `tests/import.rs`），
/// 所以这个上限再收也不能收到它下面去。
const MIN_USEFUL_DEPTH: usize = 32;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一直往下建，建到核心开口说「不能再深」为止。
///
/// 返回（已建成的 id，拒绝时那条错误）。**没有拒绝就是 bug**——调用方必须检查。
fn build_until_refused(
    store: &mut Store,
    work_id: i64,
) -> (Vec<i64>, Option<yanmo_core::Error>) {
    let mut ids = Vec::new();
    let mut parent = None;
    for _ in 0..TRY_LEVELS {
        match store.create_node(work_id, parent, NodeKind::Chapter, "") {
            Ok(id) => {
                ids.push(id);
                parent = Some(id);
            }
            Err(error) => return (ids, Some(error)),
        }
    }
    (ids, None)
}

#[test]
fn the_deepest_tree_you_can_build_is_still_readable() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "深树").unwrap();
    let (ids, refusal) = build_until_refused(&mut store, work.id);

    let refusal = refusal.expect(
        "一路建到试探上限都没被拦：入口没有深度守门，于是造出了「写得进、读不了」的树",
    );
    assert_eq!(refusal.code(), codes::TREE_TOO_DEEP, "到上限要给得出手的理由：{refusal}");
    assert!(
        ids.len() >= MIN_USEFUL_DEPTH,
        "上限别收得太小（导入承诺 {MIN_USEFUL_DEPTH} 层能进能出）：只建成 {} 层",
        ids.len()
    );

    // ① 能建出来的最深那一层，祖先链必须读得出来
    //    （界面「打开这一章沿祖先链展开」走的就是这条路）
    let deepest = *ids.last().unwrap();
    let ancestors = store.node_ancestors(deepest).expect("能写进去的最深一层必须读得出来");
    assert_eq!(ancestors.len(), ids.len() - 1, "祖先链短了");

    // ② 在最深处写正文、读回来；目录里建出来的每一层都在
    let body = "最深处那一章写的字。";
    store.write_body(deepest, body).unwrap();
    assert_eq!(store.read_body(deepest).unwrap(), body);
    let listed: Vec<i64> = store.list_nodes(work.id).unwrap().iter().map(|node| node.id).collect();
    for id in &ids {
        assert!(listed.contains(id), "目录里少了 #{id}");
    }
    assert_eq!(listed.len(), ids.len() + 1, "只该多出建书时自带的那一章");

    // ③ 汇总不许漏掉深层：根节点的合计必须把最深处那一章算进去
    let rollup = store.subtree_rollup(ids[0]).unwrap();
    assert_eq!(rollup.chapters, (ids.len() - 1) as i64, "卷行少算了深层的章节");
    assert_eq!(rollup.char_count, body.chars().count() as i64, "卷行少算了深层的字");
}

#[test]
fn moving_a_tall_subtree_under_a_deep_parent_is_refused() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "深树").unwrap();
    let (ids, _) = build_until_refused(&mut store, work.id);
    let deepest = *ids.last().unwrap();

    // 另起一棵两层的小树：只把它自己挂过去是"差一层"的算法看不出来的
    let other = store.create_node(work.id, None, NodeKind::Volume, "").unwrap();
    let inner = store.create_node(work.id, Some(other), NodeKind::Chapter, "").unwrap();
    let before = store.node_ancestors(inner).unwrap();

    let error = store
        .move_node(other, Some(deepest), 0)
        .expect_err("把两层的小树挂到最深一层下面必然越限，应当拒绝");
    assert_eq!(error.code(), codes::TREE_TOO_DEEP, "{error}");

    // 拒绝之后一个字都不许动：还在原处，祖先链没变
    assert_eq!(store.node_ancestors(inner).unwrap(), before);
    assert_eq!(store.node_ancestors(inner).unwrap().len(), 1, "它应当还挂在原来的父节点下");
}

#[test]
fn a_tree_past_the_limit_is_reported_never_silently_undercounted() {
    // 绕过写入口，直接往库里塞一个越限节点——坏数据长什么样，就按什么样造。
    // 读路径（祖先链 / 子树汇总）碰到它必须**报错**，不许少算一截还报 ok。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let mut store = Store::open(&path).unwrap();
    let work = store.create_work(WorkKind::Novel, "越限数据").unwrap();
    let (ids, _) = build_until_refused(&mut store, work.id);
    let deepest_legal = *ids.last().unwrap();

    let conn = db::open(&path).unwrap();
    conn.execute(
        "INSERT INTO nodes(work_id, parent_id, node_kind, title, sort_order, created_at, updated_at)
         VALUES(?1, ?2, 'chapter', '越限的一层', 0, 0, 0)",
        [work.id, deepest_legal],
    )
    .unwrap();
    let illegal = conn.last_insert_rowid();
    drop(conn);

    let ancestors = store.node_ancestors(illegal).expect_err("越限的那一层应当读不出来并明确报错");
    assert_eq!(ancestors.code(), codes::TREE_TOO_DEEP, "{ancestors}");

    // 汇总碰到越限的子树 = 报错；从浅一层算（那棵子树仍然完整）就必须算得对
    let over = store.subtree_rollup(ids[0]).expect_err("越限的子树不许静默少算");
    assert_eq!(over.code(), codes::TREE_TOO_DEEP, "{over}");
    let fine = store.subtree_rollup(ids[1]).expect("浅一层那棵子树是完整的，应当算得出来");
    assert_eq!(fine.chapters, (ids.len() - 2) as i64 + 1, "越限那一层也算进了它这一棵");
}
