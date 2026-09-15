//! 自动分卷的验收：**成卷 / 撤卷只动结构，正文一个字都不许动**。
//!
//! 四种形态各走一遍（零层级收拢 / 卷内分出新卷 / 提前收卷起新卷第一章 / 撤卷还原），
//! 外加阈值与提议那几条纯规则在这本书上的实际取值。
//!
//! 判据都落在"看得见的结果"上：阅读顺序（走界面那套"下一章"链路取）、同级序号密集、
//! 正文逐字节一致——**不检查实现细节**。

use yanmo_core::error::codes;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;
use yanmo_core::volume::VolumePhase;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 造一本**零层级**长篇：建书时核心会给一个空卷，先把它收走，再把章排在根上。
///
/// 这就是"作者一直顺着写、还没分过卷"的样子（也正好是验收要的第一种起点）。
fn loose_novel(count: usize) -> (tempfile::TempDir, Store, i64, Vec<i64>) {
    let (dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let shell = store.list_nodes(work.id).unwrap()[0].id;
    store.soft_delete_node(shell).unwrap();
    let mut chapters = Vec::new();
    for at in 0..count {
        let id = store.create_node(work.id, None, NodeKind::Chapter, "").unwrap();
        store.write_body(id, &format!("第{}章的正文。", at + 1)).unwrap();
        chapters.push(id);
    }
    (dir, store, work.id, chapters)
}

/// 一本长篇：第一卷里排 `count` 章（建书给的那个空卷当第一卷）。
fn volume_novel(count: usize) -> (tempfile::TempDir, Store, i64, i64, Vec<i64>) {
    let (dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for at in 0..count {
        let id = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
        store.write_body(id, &format!("第{}章的正文。", at + 1)).unwrap();
        chapters.push(id);
    }
    (dir, store, work.id, volume, chapters)
}

fn ids(store: &Store, work_id: i64, parent: Option<i64>) -> Vec<i64> {
    store.children_of(work_id, parent).unwrap().iter().map(|node| node.id).collect()
}

/// 阅读顺序：**只走界面那套"下一章"链路**（不借内部遍历，免得自证清白）。
fn reading_order(store: &Store, work_id: i64) -> Vec<i64> {
    let chapters: Vec<i64> = store
        .list_nodes(work_id)
        .unwrap()
        .into_iter()
        .filter(|node| node.kind == NodeKind::Chapter)
        .map(|node| node.id)
        .collect();
    let first = chapters
        .into_iter()
        .find(|id| store.chapter_neighbors(*id).unwrap().previous.is_none());
    let mut out = Vec::new();
    let mut cursor = first;
    while let Some(id) = cursor {
        out.push(id);
        cursor = store.chapter_neighbors(id).unwrap().next.map(|next| next.id);
    }
    out
}

fn bodies(store: &Store, chapters: &[i64]) -> Vec<String> {
    chapters.iter().map(|id| store.read_body(*id).unwrap()).collect()
}

/// 每一层的同级序号必须是密集的 `0..n-1`（成卷 / 撤卷都得守住这条不变量）。
fn assert_dense(store: &Store, work_id: i64) {
    let mut by_parent: std::collections::BTreeMap<Option<i64>, Vec<i64>> =
        std::collections::BTreeMap::new();
    for node in store.list_nodes(work_id).unwrap() {
        by_parent.entry(node.parent_id).or_default().push(node.sort_order);
    }
    for (parent, orders) in by_parent.iter_mut() {
        orders.sort_unstable();
        let expected: Vec<i64> = (0..orders.len() as i64).collect();
        assert_eq!(*orders, expected, "父 {parent:?} 下的同级序号该是密集的");
    }
}

#[test]
fn a_zero_level_novel_can_still_form_a_volume() {
    let (_dir, mut store, work_id, chapters) = loose_novel(10);
    let before = reading_order(&store, work_id);
    assert_eq!(ids(&store, work_id, None), chapters, "一开始全散在根上");

    let receipt = store.close_volume(chapters[5], "").unwrap();
    assert_eq!(receipt.moved, 6, "上一个卷之后、到收卷点为止的散章整段收成一卷");
    assert_eq!(receipt.opened_chapter, None, "后面还有章：光标不用另起一章");
    assert_eq!(
        ids(&store, work_id, None),
        vec![receipt.volume_id, chapters[6], chapters[7], chapters[8], chapters[9]],
        "新卷落在原来那一段散章的位置上，后面的章还在根上"
    );
    assert_eq!(ids(&store, work_id, Some(receipt.volume_id)), chapters[..6].to_vec());
    assert_eq!(reading_order(&store, work_id), before, "阅读顺序一个都不许变");
    assert_dense(&store, work_id);
    // 号是位置的函数：卷里各数各的，这一卷的第 6 章仍渲染成「第6章」
    assert_eq!(store.rendered_title(chapters[5]).unwrap(), "第6章");
}

#[test]
fn closing_inside_a_volume_splits_the_tail_into_the_next_volume() {
    let (_dir, mut store, work_id, volume_one, chapters) = volume_novel(30);
    let before_order = reading_order(&store, work_id);
    let before_bodies = bodies(&store, &chapters);

    store.set_volume_target(work_id, Some(30)).unwrap();
    let spot = store.volume_offer(chapters[23]).unwrap().expect("第一卷里到第 24 章正是收卷点");
    assert_eq!(spot.container, Some(volume_one), "收卷点落在第一卷里");
    assert_eq!(spot.offer.count, 24);

    let receipt = store.close_volume(chapters[23], "").unwrap();
    assert_eq!(receipt.moved, 6, "收卷点之后的 6 章整体归新卷");
    let roots = ids(&store, work_id, None);
    assert_eq!(roots, vec![volume_one, receipt.volume_id], "新卷紧跟在当前卷后面");
    assert_eq!(ids(&store, work_id, Some(volume_one)), chapters[..24].to_vec());
    assert_eq!(ids(&store, work_id, Some(receipt.volume_id)), chapters[24..].to_vec());
    assert_eq!(store.subtree_rollup(receipt.volume_id).unwrap().chapters, 6, "本卷 6 章");
    assert_eq!(reading_order(&store, work_id), before_order);
    assert_eq!(bodies(&store, &chapters), before_bodies, "成卷不碰正文");
    // 号怎么数**跟设置走**（默认跨卷延续）：新卷第一张接着第一卷数下去
    assert_eq!(store.rendered_title(chapters[24]).unwrap(), "第25章");
    assert_dense(&store, work_id);
}

#[test]
fn closing_early_opens_the_next_volume_with_its_first_chapter() {
    let (_dir, mut store, work_id, volume_one, chapters) = volume_novel(24);
    store.set_volume_target(work_id, Some(30)).unwrap();
    let before_bodies = bodies(&store, &chapters);

    let receipt = store.close_volume(chapters[23], "").unwrap();
    assert_eq!(receipt.moved, 0, "收卷点就是最后一章：没有章要搬");
    let opened = receipt.opened_chapter.expect("空卷写不了字，得顺手起第一章让光标落过去");
    assert_eq!(ids(&store, work_id, Some(receipt.volume_id)), vec![opened]);
    assert_eq!(store.rendered_title(opened).unwrap(), "第25章", "默认跨卷延续：接着第一卷数");
    assert_eq!(ids(&store, work_id, Some(volume_one)).len(), 24);
    assert_eq!(reading_order(&store, work_id), [chapters.clone(), vec![opened]].concat());
    assert_eq!(bodies(&store, &chapters), before_bodies, "成卷不碰正文");
    assert_dense(&store, work_id);
}

#[test]
fn after_the_first_volume_the_next_close_starts_the_one_after_it() {
    let (_dir, mut store, work_id, chapters) = loose_novel(6);
    let first = store.close_volume(chapters[5], "").unwrap();
    assert_eq!(ids(&store, work_id, None), vec![first.volume_id]);

    // 接着写：新章跟当前章同父（进了第一卷）——这正是界面「+」的行为
    let mut more = Vec::new();
    for _ in 0..4 {
        let after = more.last().copied().unwrap_or(chapters[5]);
        more.push(store.add_chapter_after(after, NodeKind::Chapter, "").unwrap());
    }
    assert_eq!(
        ids(&store, work_id, Some(first.volume_id)),
        [chapters.clone(), more.clone()].concat(),
        "继续写的章落在第一卷里"
    );

    let second = store.close_volume(more[3], "").unwrap();
    let opened = second.opened_chapter.expect("这次也是在章末收卷");
    assert_eq!(ids(&store, work_id, Some(first.volume_id)).len(), 10, "第一卷定在 10 章");
    assert_eq!(ids(&store, work_id, Some(second.volume_id)), vec![opened]);
    assert_eq!(ids(&store, work_id, None), vec![first.volume_id, second.volume_id]);
    assert_dense(&store, work_id);
}

#[test]
fn dissolving_undoes_both_shapes_of_close() {
    // ① 根层收拢 → 撤卷后散回根上，顺序不变
    let (_dir, mut store, work_id, chapters) = loose_novel(10);
    let before = reading_order(&store, work_id);
    let receipt = store.close_volume(chapters[5], "").unwrap();
    let undo = store.dissolve_volume(receipt.volume_id).unwrap();
    assert_eq!(undo.moved, 6);
    assert_eq!(undo.merged_into, None, "前面没有卷：抬回父层原来那一位");
    assert_eq!(ids(&store, work_id, None), chapters);
    assert_eq!(reading_order(&store, work_id), before);
    assert!(
        !store.list_nodes(work_id).unwrap().iter().any(|node| node.id == receipt.volume_id),
        "树上不该再有它"
    );
    assert!(
        store.list_trash().unwrap().iter().any(|entry| entry.id == receipt.volume_id),
        "软删进回收站：撤销之后反悔还捞得回来"
    );
    assert_dense(&store, work_id);

    // ② 卷内分出新卷 → 撤卷后并回上一卷（正好是那一步的逆操作）
    let (_dir, mut store, work_id, volume_one, chapters) = volume_novel(30);
    let before_order = reading_order(&store, work_id);
    let before_bodies = bodies(&store, &chapters);
    let receipt = store.close_volume(chapters[23], "").unwrap();
    let undo = store.dissolve_volume(receipt.volume_id).unwrap();
    assert_eq!(undo.moved, 6);
    assert_eq!(undo.merged_into, Some(volume_one), "并回第一卷末尾");
    assert_eq!(ids(&store, work_id, None), vec![volume_one]);
    assert_eq!(ids(&store, work_id, Some(volume_one)), chapters, "30 章回到同一卷里");
    assert_eq!(reading_order(&store, work_id), before_order, "阅读顺序不变");
    assert_eq!(bodies(&store, &chapters), before_bodies, "撤卷也不碰正文");
    assert_dense(&store, work_id);
}

#[test]
fn an_offer_needs_a_ruler_and_starts_at_eighty_percent() {
    let (_dir, mut store, work_id, chapters) = loose_novel(40);
    assert!(
        store.volume_offer(chapters[30]).unwrap().is_none(),
        "既没设过、也还没有历史：不拿一把通用尺子量他"
    );

    store.set_volume_target(work_id, Some(30)).unwrap();
    assert!(store.volume_offer(chapters[22]).unwrap().is_none(), "80% 之前不打扰");
    let early = store.volume_offer(chapters[23]).unwrap().expect("第 24 章是 80%");
    assert_eq!(early.offer.count, 24);
    assert_eq!(early.offer.phase, VolumePhase::Early);
    assert_eq!(early.offer.plan.effective, Some(30));
    assert_eq!(early.offer.plan.learned, None, "还没收过卷，学不到东西");
    assert_eq!(early.container, None, "还散在根上，不属于任何一卷");
    assert_eq!(
        store.volume_offer(chapters[29]).unwrap().unwrap().offer.phase,
        VolumePhase::OnTarget
    );
    assert_eq!(store.volume_offer(chapters[35]).unwrap().unwrap().offer.phase, VolumePhase::Late);
    assert_eq!(
        store.volume_offer(chapters[39]).unwrap().unwrap().offer.count,
        40,
        "过了 120% 也给入口"
    );
}

#[test]
fn the_threshold_learns_from_his_own_finished_volumes() {
    // 三卷：22 / 24 / 2 章。最后一卷正在写 → 历史 = [22, 24] → 中位数 23
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume_one = store.list_nodes(work.id).unwrap()[0].id;
    let volume_two = store.create_node(work.id, None, NodeKind::Volume, "").unwrap();
    let volume_three = store.create_node(work.id, None, NodeKind::Volume, "").unwrap();
    for (parent, count) in [(volume_one, 22), (volume_two, 24)] {
        for _ in 0..count {
            store.create_node(work.id, Some(parent), NodeKind::Chapter, "").unwrap();
        }
    }
    let mut writing = Vec::new();
    for _ in 0..21 {
        writing.push(store.create_node(work.id, Some(volume_three), NodeKind::Chapter, "").unwrap());
    }

    let plan = store.volume_plan(work.id).unwrap();
    assert_eq!(plan.target, None, "他没设过「大概几章一卷」");
    assert_eq!(plan.learned, Some(23), "收好的两卷是 22 / 24：中位数 23");
    assert_eq!(plan.effective, Some(23), "学过之后按他的历史算，不按通用尺子");
    // 阈值 23 的 80% 是 18.4 → 第 19 章起就该提
    assert!(store.volume_offer(writing[17]).unwrap().is_none(), "第 18 章还差一点");
    let offer = store.volume_offer(writing[18]).unwrap().expect("第 19 章到了 80%");
    assert_eq!(offer.container, Some(volume_three), "正在写的是第三卷");
    assert_eq!(offer.offer.count, 19);
    assert_eq!(offer.offer.phase, VolumePhase::Early);
    assert_eq!(offer.offer.plan.learned, Some(23));
}

#[test]
fn a_volume_that_is_already_closed_stops_asking() {
    // 第一卷收在 24 章、第二卷接着写：再回到第一卷最后一章时**不该再问**
    // （在那里收会往两卷之间塞进一个新卷，第二卷的章被挤到新卷后面去）
    let (_dir, mut store, work_id, _volume_one, chapters) = volume_novel(30);
    store.set_volume_target(work_id, Some(30)).unwrap();
    let second = store.close_volume(chapters[23], "").unwrap();
    assert_eq!(ids(&store, work_id, Some(second.volume_id)), chapters[24..].to_vec());

    let spot = store.volume_offer(chapters[23]).unwrap();
    assert!(spot.is_none(), "第一卷已经收好了：不能再提一次");
    // 正在写的第二卷仍然会提（它才是"还没收"的那一卷）
    assert!(store.volume_offer(chapters[24]).unwrap().is_none(), "第二卷才 1 章，离阈值还远");
    // 卷长口径也要把第一卷算进历史里
    assert_eq!(store.volume_plan(work_id).unwrap().effective, Some(30));
}

#[test]
fn closing_is_refused_where_it_makes_no_sense() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let section = store.create_node(work.id, Some(chapter), NodeKind::Section, "").unwrap();

    assert_eq!(
        store.close_volume(volume, "").unwrap_err().code(),
        codes::VOLUME_CLOSE_POINT,
        "卷自己不是收卷点"
    );
    assert_eq!(
        store.close_volume(section, "").unwrap_err().code(),
        codes::VOLUME_CLOSE_POINT,
        "节这一层收不了卷"
    );
    assert!(store.volume_offer(section).unwrap().is_none(), "提议这一侧也不提");
    assert!(store.volume_offer(volume).unwrap().is_none());
    assert_eq!(
        store.dissolve_volume(chapter).unwrap_err().code(),
        codes::VOLUME_NOT_VOLUME,
        "撤卷只认卷"
    );
    assert_eq!(
        store.close_volume(999_999, "").unwrap_err().code(),
        codes::NODE_GONE,
        "不存在的节点要明确报错"
    );
}

#[test]
fn a_close_volume_that_fails_halfway_leaves_no_stray_volume() {
    // 2026-09-15 代码质量评审：严重 4。收卷以前是"每个 create/move 各自提交"：中途任何一步失败，
    // 新卷已经建好并挪了位置、一部分章还没进去，作者看到的是一个半成品。
    //
    // 这里用"老版本留下的超深子树"制造中途失败（与 tests/deep_tree.rs 同一手法：绕过写入口、
    // 用原始 SQL 造出写入口不允许的深度——真实来源就是"深度守门加进来之前建的树"）。
    // 搬那一棵会越限、必然失败；而**新卷在这之前已经建好了**——正是要验证"失败时它也得消失"。
    let (dir, mut store, work_id, volume, chapters) = volume_novel(3);
    let close_point = chapters[0];
    let deep = chapters[1];

    let path = dir.path().join("yanmo.db");
    let conn = yanmo_core::db::open(&path).unwrap();
    let mut parent = deep;
    for level in 0..63 {
        conn.execute(
            &format!(
                "INSERT INTO nodes(work_id, parent_id, node_kind, title, sort_order, created_at, updated_at) \
                 VALUES({work_id}, {parent}, 'chapter', '很深的一层 {level}', 0, 0, 0)"
            ),
            [],
        )
        .unwrap();
        parent = conn.last_insert_rowid();
    }
    drop(conn);

    let roots_before: Vec<i64> =
        store.list_nodes(work_id).unwrap().iter().filter(|n| n.parent_id.is_none()).map(|n| n.id).collect();
    let under_volume_before: Vec<i64> = store
        .list_nodes(work_id)
        .unwrap()
        .iter()
        .filter(|n| n.parent_id == Some(volume))
        .map(|n| n.id)
        .collect();
    assert_eq!(under_volume_before.len(), 3, "前提：三章都还在这一卷里");

    let error = store.close_volume(close_point, "第二卷").expect_err("搬一棵 64 层的子树必然越限");
    assert_eq!(error.code(), codes::TREE_TOO_DEEP, "{error}");

    let roots_after: Vec<i64> =
        store.list_nodes(work_id).unwrap().iter().filter(|n| n.parent_id.is_none()).map(|n| n.id).collect();
    let under_volume_after: Vec<i64> = store
        .list_nodes(work_id)
        .unwrap()
        .iter()
        .filter(|n| n.parent_id == Some(volume))
        .map(|n| n.id)
        .collect();

    assert_eq!(roots_after, roots_before, "失败之后不许留下一个建好一半的新卷");
    assert_eq!(under_volume_after, under_volume_before, "原来的章还得在原处");
}
