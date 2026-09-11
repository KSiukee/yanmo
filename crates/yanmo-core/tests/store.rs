//! 存储层验收：**零层级单篇文章**与**多层长篇**都能建、能写、能读、能搜。
//!
//! 这两条形态不是"顺手测一下"——它们正是「结构深度不写死」的判据：
//! 只要有一种形态建不出来，就说明代码里偷偷假设了层级。

use std::time::Duration;

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 小睡一下：多处逻辑按毫秒时间戳排序，测试里别让两次操作撞在同一毫秒。
fn tick() {
    std::thread::sleep(Duration::from_millis(3));
}

fn op_log_count(store: &Store) -> i64 {
    store
        .conn()
        .query_row("SELECT COUNT(*) FROM op_log", [], |r| r.get(0))
        .unwrap()
}

fn titles(store: &Store, work_id: i64) -> Vec<String> {
    store.list_nodes(work_id).unwrap().iter().map(|n| n.title.clone()).collect()
}

#[test]
fn article_is_zero_level_and_works_end_to_end() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "我的第一篇").unwrap();

    let nodes = store.list_nodes(work.id).unwrap();
    assert_eq!(nodes.len(), 1, "零层级文章应当只有一个根节点");
    let piece = &nodes[0];
    assert_eq!(piece.kind, NodeKind::Piece);
    assert_eq!(piece.parent_id, None, "根节点即正文，没有父级");
    assert_eq!(piece.title, "我的第一篇");
    assert!(!piece.has_body, "还没写正文");
    assert_eq!(piece.word_count, 0);

    let body = "这是一篇零层级的文章。它没有卷，也没有章。";
    let stats = store.write_body(piece.id, body).unwrap();
    assert_eq!(stats.char_count, yanmo_core::text::count_chars(body));
    assert!(stats.word_count > 0);
    assert_eq!(store.read_body(piece.id).unwrap(), body, "写进去的必须能原样读回");

    let nodes = store.list_nodes(work.id).unwrap();
    assert!(nodes[0].has_body);
    assert_eq!(nodes[0].word_count, stats.word_count, "目录树字数应来自写入时回算");

    // 检索：≥3 字走索引，2 字短词走回退路径——两条都要能用
    let long_hits = store.search("零层级的文章", None, 10).unwrap();
    assert_eq!(long_hits.len(), 1);
    assert_eq!(long_hits[0].node_id, piece.id);
    let short_hits = store.search("文章", None, 10).unwrap();
    assert_eq!(short_hits.len(), 1, "两个字的中文词不能搜不到");
    assert!(short_hits[0].snippet.contains("文章"));
}

#[test]
fn novel_supports_deep_tree_and_layer_by_layer_queries() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    let root = store.list_nodes(work.id).unwrap()[0].clone();
    assert_eq!(root.kind, NodeKind::Volume);
    assert_eq!(root.title, "第一卷");

    let ch1 = store.create_node(work.id, Some(root.id), NodeKind::Chapter, "第一章").unwrap();
    let ch2 = store.create_node(work.id, Some(root.id), NodeKind::Chapter, "第二章").unwrap();
    let scene = store.create_node(work.id, Some(ch1), NodeKind::Scene, "雨夜").unwrap();
    let section = store.create_node(work.id, Some(scene), NodeKind::Section, "细节").unwrap();
    assert!(ch1 > 0 && ch2 > 0 && scene > 0 && section > 0);

    // 懒加载：展开哪一层拉哪一层
    assert_eq!(store.children_of(work.id, None).unwrap().len(), 1);
    let chapters = store.children_of(work.id, Some(root.id)).unwrap();
    assert_eq!(
        chapters.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
        vec!["第一章", "第二章"]
    );
    assert_eq!(chapters[0].sort_order, 0);
    assert_eq!(chapters[1].sort_order, 1);
    assert_eq!(store.children_of(work.id, Some(ch1)).unwrap()[0].id, scene);
    assert_eq!(store.children_of(work.id, Some(scene)).unwrap()[0].id, section);

    // 一次拉全树也要成立（四层）
    assert_eq!(store.list_nodes(work.id).unwrap().len(), 5);

    // 只有写过正文的节点才有字数和 has_body——树查询不碰正文
    store.write_body(ch1, "雨下了一整夜。").unwrap();
    let chapters = store.children_of(work.id, Some(root.id)).unwrap();
    assert!(chapters[0].has_body && chapters[0].word_count > 0);
    assert!(!chapters[1].has_body && chapters[1].word_count == 0);

    // 场景卡里的字也能搜到（叶子节点一样进索引）
    store.write_body(scene, "他把铜钱按在桌上。").unwrap();
    assert_eq!(store.search("铜钱", None, 10).unwrap()[0].node_id, scene);
}

#[test]
fn move_reorders_densely_and_refuses_cycles_and_cross_work() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    let a = store.create_node(work.id, Some(root), NodeKind::Chapter, "第一章").unwrap();
    let b = store.create_node(work.id, Some(root), NodeKind::Chapter, "第二章").unwrap();
    let c = store.create_node(work.id, Some(root), NodeKind::Chapter, "第三章").unwrap();

    store.move_node(c, Some(root), 0).unwrap();
    let chapters = store.children_of(work.id, Some(root)).unwrap();
    assert_eq!(
        chapters.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
        vec!["第三章", "第一章", "第二章"]
    );
    assert_eq!(
        chapters.iter().map(|n| n.sort_order).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "同级必须是密集序号，不留空洞"
    );

    // 越界索引夹到末尾
    store.move_node(c, Some(root), 99).unwrap();
    let chapters = store.children_of(work.id, Some(root)).unwrap();
    assert_eq!(chapters.last().unwrap().id, c);

    // 不能把节点移进自己的子孙（成环）
    assert!(store.move_node(root, Some(a), 0).is_err(), "移进自己的子孙必须被拒绝");
    assert!(store.move_node(a, Some(a), 0).is_err(), "自己不能当自己的父级");

    // 不能跨作品挂载
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    let other_root = store.list_nodes(other.id).unwrap()[0].id;
    assert!(store.move_node(a, Some(other_root), 0).is_err(), "跨作品移动必须被拒绝");
    // 但本作品内的合法移动照常：把第二章提到根级（两个父级都要重排）
    store.move_node(b, None, 0).unwrap();
    assert_eq!(store.children_of(work.id, None).unwrap().len(), 2, "根级多了一个节点");
    assert_eq!(
        store
            .children_of(work.id, Some(root))
            .unwrap()
            .iter()
            .map(|n| n.title.as_str())
            .collect::<Vec<_>>(),
        vec!["第一章", "第三章"]
    );
    // 全树列表：根级在前（父级为空的先出），同级按密集序号
    assert_eq!(
        titles(&store, work.id),
        vec!["第二章", "第一卷", "第一章", "第三章"]
    );
}

#[test]
fn unchanged_content_is_a_no_op() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    let piece = store.list_nodes(work.id).unwrap()[0].id;

    let first = store.write_body(piece, "重复写同样内容。").unwrap();
    let ops = op_log_count(&store);
    tick();
    let again = store.write_body(piece, "重复写同样内容。").unwrap();
    assert_eq!(again, first, "内容没变，字数口径也应一致");
    assert_eq!(op_log_count(&store), ops, "内容没变就不该产生任何写入与日志");

    store.write_body(piece, "改一个字。").unwrap();
    assert_eq!(op_log_count(&store), ops + 1, "内容变了才写库、才留痕");
}

#[test]
fn soft_delete_hides_subtree_from_tree_and_search() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    let ch = store.create_node(work.id, Some(root), NodeKind::Chapter, "第一章").unwrap();
    let scene = store.create_node(work.id, Some(ch), NodeKind::Scene, "雨夜").unwrap();
    store.write_body(ch, "雨夜，铜钱落在桌上。").unwrap();

    assert_eq!(store.search("铜钱", None, 10).unwrap().len(), 1);

    // 删父节点连带整棵子树（这里 2 个）
    assert_eq!(store.soft_delete_node(ch).unwrap(), 2);
    assert_eq!(store.list_nodes(work.id).unwrap().len(), 1, "只剩根节点");
    assert!(store.search("铜钱", None, 10).unwrap().is_empty(), "删掉的不能还能搜到");
    assert!(store.write_body(ch, "往回收站里写东西应当被拒绝").is_err());
    assert!(store.write_body(scene, "子节点也一样").is_err());
    assert_eq!(store.read_body(ch).unwrap(), "雨夜，铜钱落在桌上。", "正文仍在——回收站要能还原");

    // 作品软删后，整棵树都不再出现
    store.soft_delete_work(work.id).unwrap();
    assert!(store.list_nodes(work.id).unwrap().is_empty());
    assert!(store.search("铜钱", None, 10).unwrap().is_empty());
    assert!(store.rename_work(work.id, "改名").is_err(), "已删除的作品不能再改");
}

#[test]
fn search_tracks_title_edits_and_reindex_rebuilds_it() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    let piece = store.list_nodes(work.id).unwrap()[0].id;

    store.rename_node(piece, "铜钱之谜").unwrap();
    let hits = store.search("铜钱", None, 10).unwrap();
    assert_eq!(hits.len(), 1, "改标题后索引必须跟着走（触发器负责）");
    assert!(hits[0].matched_title);

    store.write_body(piece, "他把那枚铜钱收进袖子里。").unwrap();
    let hits = store.search("铜钱", None, 10).unwrap();
    assert_eq!(hits.len(), 1, "同一条命中不该因为标题+正文各命中一次就出现两行");
    assert!(hits[0].snippet.contains("铜钱"), "片段应指向正文命中处");

    // 人为清空索引 → 搜不到（证明检索确实走索引，不是全表扫）
    store.conn().execute("DELETE FROM node_fts", []).unwrap();
    assert!(store.search("铜钱", None, 10).unwrap().is_empty());

    // 重建 → 恢复
    assert_eq!(store.reindex().unwrap(), 1);
    assert_eq!(store.search("铜钱", None, 10).unwrap().len(), 1);
}

#[test]
fn shelf_orders_by_recent_open_and_op_log_records_writes() {
    let (_dir, mut store) = fresh();
    let a = store.create_work(WorkKind::Article, "A").unwrap();
    tick();
    let b = store.create_work(WorkKind::Article, "B").unwrap();
    assert_eq!(
        store.list_works().unwrap().iter().map(|w| w.title.clone()).collect::<Vec<_>>(),
        vec!["B", "A"],
        "刚建的排最前"
    );

    tick();
    store.touch_work_opened(a.id).unwrap();
    assert_eq!(
        store.list_works().unwrap().iter().map(|w| w.title.clone()).collect::<Vec<_>>(),
        vec!["A", "B"],
        "最近打开的排最前"
    );

    store.soft_delete_work(b.id).unwrap();
    assert_eq!(store.list_works().unwrap().len(), 1);

    store.rename_work(a.id, "A2").unwrap();
    let piece = store.list_nodes(a.id).unwrap()[0].id;
    store.write_body(piece, "正文。").unwrap();

    let device = store.device_id().to_string();
    assert!(!device.is_empty());
    let mut stmt = store
        .conn()
        .prepare("SELECT seq, device_id, entity, op FROM op_log ORDER BY seq")
        .unwrap();
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(rows.len() >= 6, "每次写操作都该留下痕迹：{rows:?}");
    assert!(rows.windows(2).all(|w| w[1].0 > w[0].0), "op-log 序号必须单调递增");
    assert!(rows.iter().all(|r| r.1 == device), "每条日志都要署名本机设备");
    let ops: Vec<&str> = rows.iter().map(|r| r.3.as_str()).collect();
    for expected in ["create", "rename", "write", "delete"] {
        assert!(ops.contains(&expected), "缺少 op={expected}：{ops:?}");
    }
}

#[test]
fn previous_schema_database_is_upgraded_and_search_backfilled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");

    // 造一个"上一版引擎留下的库"：只跑 v1，直接塞数据——那时还没有检索索引
    {
        let conn = yanmo_core::db::open(&path).unwrap();
        for step in yanmo_core::db::migrations::MIGRATIONS[0].steps {
            conn.execute(step, []).unwrap();
        }
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO works(id, kind, title, created_at, updated_at, opened_at)
             VALUES(1, 'article', '旧稿', 1, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id, work_id, parent_id, node_kind, title, sort_order, word_count,
                 created_at, updated_at)
             VALUES(1, 1, NULL, 'piece', '旧稿', 0, 0, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO node_contents(node_id, body, content_hash, char_count, updated_at)
             VALUES(1, '旧库里已经有正文了。', '', 0, 1)",
            [],
        )
        .unwrap();
    }

    // 新版打开：结构自动升级，**存量正文要回填进索引**（否则老稿子升级后搜不到）
    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version(),
        "打开旧库应当升到最新结构版本"
    );
    assert_eq!(store.read_body(1).unwrap(), "旧库里已经有正文了。");
    let hits = store.search("已经有正文", None, 10).unwrap();
    assert_eq!(hits.len(), 1, "升级后老稿子必须立刻能搜到（存量回填）");
    assert_eq!(hits[0].node_id, 1);
}

#[test]
fn subtree_rollup_counts_chapters_and_words_per_container() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume_one = store.list_nodes(work.id).unwrap()[0].id;
    let volume_two = store.create_node(work.id, None, NodeKind::Volume, "第二卷").unwrap();

    let mut write = |parent: i64, title: &str, body: &str| {
        let id = store.create_node(work.id, Some(parent), NodeKind::Chapter, title).unwrap();
        store.write_body(id, body).unwrap();
        id
    };
    let first = write(volume_one, "第一章", "一二三四五");
    write(volume_one, "第二章", "六七");
    write(volume_two, "第三章", "八九十");

    // 场景卡挂在第一章下：它算字数，但不算"章"
    let scene = store
        .create_node(work.id, Some(first), NodeKind::Scene, "场景卡")
        .unwrap();
    store.write_body(scene, "场景卡里的备注。").unwrap();

    let one = store.subtree_rollup(volume_one).unwrap();
    let two = store.subtree_rollup(volume_two).unwrap();
    assert_eq!(one.chapters, 2, "第一卷两章");
    assert_eq!(two.chapters, 1, "第二卷一章");

    // 汇总的数字必须与"把子树里每个节点的字数加起来"一致——两边不许各算一套
    let all = store.list_nodes(work.id).unwrap();
    let mut expected = 0;
    for node in all.iter().filter(|n| n.parent_id == Some(volume_one)) {
        expected += node.word_count;
        for kid in all.iter().filter(|n| n.parent_id == Some(node.id)) {
            expected += kid.word_count;
        }
    }
    assert_eq!(one.word_count, expected, "本卷字数 = 子树里各节点字数之和");
    assert!(one.word_count > two.word_count, "第一卷多一章还有场景卡");

    // 软删一章：汇总立刻跟着少
    let second = all.iter().find(|n| n.title == "第二章").unwrap().id;
    store.soft_delete_node(second).unwrap();
    let after = store.subtree_rollup(volume_one).unwrap();
    assert_eq!(after.chapters, 1, "删掉的章不算了");
    assert!(after.word_count < one.word_count);
}

#[test]
fn volume_target_is_per_work_and_clears_cleanly() {
    let (_dir, mut store) = fresh();
    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let other = store.create_work(WorkKind::Novel, "短歌").unwrap();

    assert_eq!(store.volume_target(novel.id).unwrap(), None, "没设过就是没有，不猜默认值");
    store.set_volume_target(novel.id, Some(30)).unwrap();
    assert_eq!(store.volume_target(novel.id).unwrap(), Some(30));
    assert_eq!(store.volume_target(other.id).unwrap(), None, "每部作品各记各的");

    // ≤0 与 None 都当"清掉"：这是行小字，不该因为它把界面卡住
    store.set_volume_target(novel.id, Some(0)).unwrap();
    assert_eq!(store.volume_target(novel.id).unwrap(), None);
    store.set_volume_target(novel.id, Some(12)).unwrap();
    store.set_volume_target(novel.id, None).unwrap();
    assert_eq!(store.volume_target(novel.id).unwrap(), None);

    // 已删除的作品不给设
    store.soft_delete_work(novel.id).unwrap();
    assert!(store.set_volume_target(novel.id, Some(5)).is_err());
}

#[test]
fn default_chapter_name_skips_numbers_already_in_use() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;

    let mut ids = Vec::new();
    for _ in 0..3 {
        let id = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
        ids.push(id);
    }
    let chapters = |store: &Store| -> Vec<String> {
        store
            .list_nodes(work.id)
            .unwrap()
            .into_iter()
            .filter(|n| n.kind == NodeKind::Chapter)
            .map(|n| n.title)
            .collect()
    };
    assert_eq!(chapters(&store), vec!["第1章", "第2章", "第3章"], "取号从 1 开始，一个不跳");

    // ★ 删中间的第二章：再新建**不能**又算出"第3章"（那会跟还在的第三章撞名）
    store.soft_delete_node(ids[1]).unwrap();
    let created = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let titles = chapters(&store);
    assert_eq!(
        store.node_title(created).unwrap(),
        "第4章",
        "取的是**已用过的最大号 + 1**，不是「现有几章 + 1」"
    );
    assert_eq!(
        titles.iter().filter(|t| *t == "第3章").count(),
        1,
        "目录里不该出现两个第3章：{titles:?}"
    );

    // 作者自起的名字一个都不动
    store.rename_node(created, "引子").unwrap();
    let next = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    assert_eq!(store.node_title(next).unwrap(), "第4章", "自起的名字不参与取号，不会把它顶到第5章");
}

/// 作者问过的那个流程：删掉中间的章 → 新建 → 改名把号补回来 → 再新建拿几号？
///
/// 取号看的是"这一层**现在**用着哪些号"（只数活着的）：
/// 把一个号腾出来，它就会被再用；没腾出来，就接着往后走。
#[test]
fn a_number_freed_by_renaming_gets_reused() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapters = |store: &Store| -> Vec<String> {
        store
            .list_nodes(work.id)
            .unwrap()
            .into_iter()
            .filter(|n| n.kind == NodeKind::Chapter)
            .map(|n| n.title)
            .collect()
    };
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap());
    }
    assert_eq!(chapters(&store), vec!["第1章", "第2章", "第3章"]);

    // ① 删掉中间的第二章，再新建：拿到第4章（不能跟还在的第3章撞）
    store.soft_delete_node(ids[1]).unwrap();
    let fresh = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    assert_eq!(store.node_title(fresh).unwrap(), "第4章");

    // ② 作者把这一章改名成「第2章」、挪回第二位 —— 号就腾出来了
    store.rename_node(fresh, "第2章").unwrap();
    store.move_node(fresh, Some(volume), 1).unwrap();
    assert_eq!(chapters(&store), vec!["第1章", "第2章", "第3章"]);

    // ③ 再新建：**又是第4章**（4 已经不占用了）
    let again = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    assert_eq!(store.node_title(again).unwrap(), "第4章", "腾出来的号会被再用");
}

#[test]
fn a_number_held_by_a_trashed_chapter_is_skipped() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap());
    }

    // 删掉第二章、**不**改名：新建拿第4章，再新建拿第5章——2 这个号一直留着（那章还在回收站里）
    store.soft_delete_node(ids[1]).unwrap();
    let fourth = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    let fifth = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    assert_eq!(store.node_title(fourth).unwrap(), "第4章");
    assert_eq!(store.node_title(fifth).unwrap(), "第5章");

    // 回收站里那一章还是"第2章"：真恢复它，就正好撞上作者可能已经手写出来的第2章
    // （那条冲突由恢复前的预检 + 作者三选一来处理）
    let trashed: String = store
        .conn()
        .query_row("SELECT title FROM nodes WHERE id = ?1", [ids[1]], |r| r.get(0))
        .unwrap();
    assert_eq!(trashed, "第2章");
}
