//! 灵感碰撞验收：**挑对 → 引子 → 存成双源溯源的一张卡 → 撞过的不再撞 → 跳过记得住**。
//!
//! 七条判据：
//! 1. 同处的（共用锚点）排在相隔的前面；没锚点的算相隔，不慌；
//! 2. 一屏里**同一条碎片只上一场**；
//! 3. 存下的那句写成一张普通灵感卡：`derived_from` 指**主源**（小 id），
//!    `linked` 里两条 `spark:` 锚点把第二条源头也记住；原来的两条一个字不动；
//! 4. 存完这一对**不再出现**（撞过的不再撞），但池子里别的组合照旧；
//! 5. 「跳过」是**落盘**的：下一次挑对还认它；跳过的组合不进"撞过"那本账（撤销回得来）；
//! 6. 撤销跳过之后那一对回来了；
//! 7. 空句子 / 拿自己撞自己 / 两条不同书的——都当场拒。
//!
//! 另有一条口径：**关系（同处 / 相隔）由核心自己算**，留痕里能看见。

use yanmo_core::error::codes;
use yanmo_core::model::{FragmentKind, NodeKind, WorkKind};
use yanmo_core::store::{NewFragment, Store, COLLISIONS_PER_BOARD};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn book(store: &mut Store) -> (i64, i64, i64) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap().id;
    let volume = store.create_node(work, None, NodeKind::Volume, "第一卷").unwrap();
    let first = store.create_node(work, Some(volume), NodeKind::Chapter, "第一章").unwrap();
    let second = store.create_node(work, Some(volume), NodeKind::Chapter, "第二章").unwrap();
    (work, first, second)
}

fn jot(store: &mut Store, work: i64, body: &str, anchors: Vec<String>) -> i64 {
    store
        .create_fragment(
            &NewFragment {
                work_id: work,
                kind: FragmentKind::Idea,
                body: body.to_string(),
                source: "typed".to_string(),
                anchors,
            },
            "test",
        )
        .unwrap()
}

/// 一屏建议里的组合（规范写法）。
fn pairs(store: &Store, work: i64) -> Vec<(i64, i64)> {
    store
        .spark_collisions(work, None, COLLISIONS_PER_BOARD)
        .unwrap()
        .into_iter()
        .map(|c| (c.left.id.min(c.right.id), c.left.id.max(c.right.id)))
        .collect()
}

/// 一条碎片身上的溯源锚点（`spark:<id>`）。
fn source_sparks(store: &Store, id: i64) -> Vec<i64> {
    let mut stmt = store
        .conn()
        .prepare("SELECT linked FROM fragments WHERE id = ?1")
        .unwrap();
    let linked: String = stmt.query_row([id], |row| row.get(0)).unwrap();
    yanmo_core::question::anchors_of(&linked)
        .iter()
        .filter_map(|a| a.strip_prefix("spark:").and_then(|v| v.trim().parse().ok()))
        .collect()
}

#[test]
fn shared_anchors_come_first_and_the_reason_is_spelled_out() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    // 两条都在第二章（同处）；另两条一个在第一章、一个哪一章都不挂（相隔）
    let a = jot(&mut store, work, "第二章的两个念头之一", vec![format!("chapter:{second}")]);
    let b = jot(&mut store, work, "第二章的两个念头之二", vec![format!("chapter:{second}")]);
    let _c = jot(&mut store, work, "第一章留着的念头", vec![format!("chapter:{first}")]);
    let _d = jot(&mut store, work, "还没归位的念头", vec![]);

    let got = store.spark_collisions(work, None, 2).unwrap();
    assert_eq!((got[0].left.id, got[0].right.id), (a.min(b), a.max(b)), "同样强时先撞同处的那对");
    assert_eq!(got[0].relation.as_str(), "together");
    assert_eq!(got[0].shared_anchors, vec![format!("chapter:{second}")], "共用哪几条要说清");
    // 剩下那两条锚点搭不上：相隔（c 与 a、b 里的某一条）
    assert_eq!(got[1].relation.as_str(), "apart");
    assert!(got[1].shared_anchors.is_empty());
}

#[test]
fn one_spark_only_takes_one_slot_in_a_board() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    // 三条都在同一处：只够凑一对（一屏里同一条只上一场）
    jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    jot(&mut store, work, "丙", vec![format!("chapter:{first}")]);
    assert_eq!(pairs(&store, work).len(), 1);
}

#[test]
fn saving_writes_one_card_with_two_sources_and_leaves_the_originals_alone() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let a = jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "主角其实五年前就知道", vec![format!("chapter:{first}")]);

    let receipt = store
        .save_collision(a, b, "他知道真相那么久，那场雨夜就变味了", "test")
        .unwrap();
    let saved = store.fragment(receipt.spark_id).unwrap();
    assert_eq!(saved.body, "他知道真相那么久，那场雨夜就变味了");
    assert_eq!(saved.kind, FragmentKind::Idea, "碰撞卡就是一张普通的灵感卡");
    assert_eq!(saved.derived_from, Some(a.min(b)), "derived_from 指主源（小 id）");
    assert_eq!(source_sparks(&store, receipt.spark_id), vec![a.min(b), a.max(b)], "两条源头都记着");
    // 原来那两条一个字不动
    assert_eq!(store.fragment(a).unwrap().body, "雨夜那一场要收线");
    assert_eq!(store.fragment(b).unwrap().body, "主角其实五年前就知道");
}

#[test]
fn a_collided_pair_never_comes_back_but_other_pairs_still_do() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    let a = jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    let c = jot(&mut store, work, "丙", vec![format!("chapter:{second}")]);
    let d = jot(&mut store, work, "丁", vec![format!("chapter:{second}")]);

    let before = pairs(&store, work);
    assert_eq!(before.len(), 2, "四条的池子够两对：{before:?}");
    let (taken, kept) = (before[0], before[1]);
    let receipt = store
        .save_collision(taken.0, taken.1, "撞出来的一句", "test")
        .unwrap();

    let after = pairs(&store, work);
    assert!(!after.contains(&taken), "撞过的那对不再出现：{after:?}");
    assert!(after.contains(&kept), "另外那一对照旧：{after:?}");
    // 新写下的那张卡**不跟自己的源头撞回去**（那是把同一件事再想一遍）
    assert!(
        !after.iter().any(|p| p.0 == receipt.spark_id || p.1 == receipt.spark_id),
        "撞出来的卡不该立刻跟源头成对：{after:?}"
    );
    let _ = (a, b, c, d);
}

#[test]
fn skipping_is_remembered_and_can_be_undone() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    jot(&mut store, work, "丙", vec![format!("chapter:{second}")]);
    jot(&mut store, work, "丁", vec![format!("chapter:{second}")]);

    let before = pairs(&store, work);
    let (left, right) = before[0];
    let after = store.skip_collision(work, left, right, None, "test").unwrap();
    assert_eq!(after.len(), 1, "跳过一对之后剩下一对");
    assert!(pairs(&store, work).iter().all(|p| *p != (left, right)), "下一次挑对还认这个记号");

    // 跳过**不进"撞过"那本账**：撤销之后它原样回来
    store.unskip_collision(work, left, right, None, "test").unwrap();
    assert_eq!(pairs(&store, work), before, "撤销跳过就该回到原来那一屏");
}

#[test]
fn a_skipped_pair_is_not_a_collided_pair() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let a = jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    store.skip_collision(work, a, b, None, "test").unwrap();
    // 跳过之后**还能**手动把这一对存下来（作者的判断压过机制的记号）
    store.save_collision(a, b, "还是决定撞一下", "test").unwrap();
    let saved: Vec<i64> = store
        .spark_candidates(work, None, 10)
        .unwrap()
        .into_iter()
        .filter(|s| s.body == "还是决定撞一下")
        .map(|s| s.id)
        .collect();
    assert_eq!(saved.len(), 1, "存下来了");
    assert_eq!(source_sparks(&store, saved[0]), vec![a.min(b), a.max(b)]);
}

#[test]
fn empty_self_and_foreign_collisions_are_rejected() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let a = jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap().id;
    let stranger = jot(&mut store, other, "别人的", vec![]);

    assert_eq!(
        store.save_collision(a, b, "   ", "test").unwrap_err().code(),
        codes::IDEA_BODY_EMPTY
    );
    assert_eq!(
        store.save_collision(a, a, "自己跟自己", "test").unwrap_err().code(),
        codes::COLLISION_SAME_SPARK
    );
    assert_eq!(
        store.save_collision(a, stranger, "跨书", "test").unwrap_err().code(),
        codes::COLLISION_FOREIGN
    );
    // 拿问题卡当碰撞源也不许（它压根不是灵感）
    assert_eq!(
        store.save_collision(a, 9999, "不存在的", "test").unwrap_err().code(),
        codes::SPARK_NOT_FOUND
    );
}

#[test]
fn a_collision_card_joins_the_pool_like_any_other_spark() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let a = jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "主角其实五年前就知道", vec![format!("chapter:{first}")]);
    let receipt = store.save_collision(a, b, "他知道真相那么久，那场雨夜就变味了", "test").unwrap();

    // 它跟别的灵感一样进候选池，而且**继承了那两条的关联锚点**（跟当前章的关联度照算）
    let candidates = store.spark_candidates(work, Some(first), 10).unwrap();
    let saved = candidates
        .iter()
        .find(|s| s.id == receipt.spark_id)
        .expect("撞出来的那张卡也在池子里");
    assert!(saved.anchors.contains(&format!("chapter:{first}")));
    assert_eq!(saved.gravity.anchor_hits, 1, "继承了源头那一条章锚点");
    assert!(
        saved.gravity.relevance > 0.25,
        "它跟当前这一章有关（高于'命中不了'的地板）：{}",
        saved.gravity.relevance
    );
    // 它自己也能被采纳（走的是同一条路）
    let adopted = store.adopt_spark(receipt.spark_id, "event", None, "", "test").unwrap();
    assert!(adopted.event_id.is_some());
}
