//! 灵感碰撞验收：**挑对 → 引子 → 存成双源溯源的一张卡 → 撞过的不再撞 → 跳过记得住**。
//!
//! 这一版两头都能是**大纲元素**（章纲 / 伏笔 / 人物卡），所以判据里多了四条：
//!
//! 1. 三种元素各自都能当另一头，关系按**共用的章锚点**判定；
//! 2. **元素×元素**不成对（那一屏是给"你记下的东西"用的）；
//! 3. 伏笔只取**还埋着**的；元素被删 / 章纲被清空 / 伏笔收了 ⇒ 认不回那一头，如实报；
//! 4. 溯源锚点写的是**规范写法**（`spark:12` / `plan:3`），`derived_from` 只指碎片那一头。
//!
//! 另有五条与上一版相同：同处优先 / 一头只上一场 / 存下即双源且原文不动 /
//! 撞过的不再来而别的组合照旧 / 跳过落盘且能撤销。

use yanmo_core::error::codes;
use yanmo_core::model::{
    Attribute, EntityKind, FragmentKind, NewEntityCard, NewForeshadow, NodeKind, WorkKind,
};
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

/// 给某一章写一句"这一章的一句话"（章纲）。
fn plan(store: &Store, node: i64, text: &str) {
    store
        .conn()
        .execute(
            "UPDATE nodes SET summary = ?1 WHERE id = ?2",
            [text.to_string(), node.to_string()],
        )
        .unwrap();
}

/// 一条伏笔（埋在 `planted` 那一章）。
fn foreshadow(store: &mut Store, work: i64, body: &str, planted: i64) -> i64 {
    store
        .create_foreshadow(
            &NewForeshadow {
                work_id: work,
                body: body.to_string(),
                planted_node: Some(planted),
                note: String::new(),
            },
            "test",
        )
        .unwrap()
}

/// 一张人物卡 + 它在哪几章出场。
fn person(store: &mut Store, work: i64, name: &str, appears_in: &[i64]) -> i64 {
    let id = store
        .create_entity_card(
            &NewEntityCard {
                work_id: work,
                kind: EntityKind::Person,
                name: name.to_string(),
                aliases: vec![],
                attributes: vec![Attribute {
                    key: "身份".to_string(),
                    value: "守夜人".to_string(),
                }],
                note: "怕水".to_string(),
            },
            "test",
        )
        .unwrap();
    for node in appears_in {
        store.set_node_cast(*node, &[id], "test").unwrap();
    }
    id
}

/// 一屏建议里的组合（两头各自的规范写法）。
fn pairs(store: &Store, work: i64) -> Vec<String> {
    store
        .spark_collisions(work, None, COLLISIONS_PER_BOARD)
        .unwrap()
        .into_iter()
        .map(|c| format!("{}×{}", c.left.key, c.right.key))
        .collect()
}

/// 一条碎片身上的**溯源锚点**（`kind:id` 那些，按存的顺序）。
fn source_keys(store: &Store, id: i64) -> Vec<String> {
    let mut stmt = store
        .conn()
        .prepare("SELECT linked FROM fragments WHERE id = ?1")
        .unwrap();
    let linked: String = stmt.query_row([id], |row| row.get(0)).unwrap();
    yanmo_core::question::anchors_of(&linked)
        .into_iter()
        .filter(|anchor| {
            let kind = anchor.split(':').next().unwrap_or_default();
            matches!(kind, "spark" | "plan" | "foreshadow" | "entity")
        })
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
    assert_eq!(
        (got[0].left.key.clone(), got[0].right.key.clone()),
        (format!("spark:{}", a.min(b)), format!("spark:{}", a.max(b))),
        "同样强时先撞同处的那对"
    );
    assert_eq!(got[0].relation.as_str(), "together");
    assert_eq!(got[0].shared_anchors, vec![format!("chapter:{second}")], "共用哪几条要说清");
    assert_eq!(got[1].relation.as_str(), "apart");
    assert!(got[1].shared_anchors.is_empty());
}

#[test]
fn a_plan_takes_the_other_side_of_a_spark() {
    let (_dir, mut store) = fresh();
    let (work, _first, second) = book(&mut store);
    plan(&store, second, "这一章要摊牌");
    let spark = jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{second}")]);

    let got = store.spark_collisions(work, None, 3).unwrap();
    assert_eq!(got.len(), 1, "一碎片 × 一章纲：{got:?}");
    assert_eq!(got[0].left.key, format!("spark:{spark}"), "碎片那一头排在前面");
    assert_eq!(got[0].right.key, format!("plan:{second}"));
    assert_eq!(got[0].right.kind.as_str(), "plan");
    assert_eq!(got[0].right.text, "这一章要摊牌");
    assert!(got[0].right.spark.is_none(), "元素那一头不带碎片");
    assert_eq!(got[0].relation.as_str(), "together", "同在一章 = 同处");
}

#[test]
fn only_planted_foreshadows_take_the_stage() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let planted = foreshadow(&mut store, work, "那把刀还没交代来历", first);
    let collected = foreshadow(&mut store, work, "已经收过的那条线", first);
    store
        .conn()
        .execute("UPDATE foreshadows SET state = 'collected' WHERE id = ?1", [collected])
        .unwrap();
    jot(&mut store, work, "顺手记的一个念头", vec![format!("chapter:{first}")]);

    let got = store.spark_collisions(work, None, 3).unwrap();
    assert_eq!(got.len(), 1, "只该有还埋着的那条：{got:?}");
    assert_eq!(got[0].right.key, format!("foreshadow:{planted}"));
    assert_eq!(got[0].right.text, "那把刀还没交代来历");
    assert_eq!(got[0].relation.as_str(), "together", "埋在这一章 = 同处");
}

#[test]
fn an_entity_card_gets_its_chapters_from_the_cast() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    let card = person(&mut store, work, "李三", &[second]);
    // 念头挂在第一章：跟"只在第二章出场的人"搭不上（相隔），但照样能撞
    jot(&mut store, work, "第一章的一个念头", vec![format!("chapter:{first}")]);
    // 第二个念头挂在第二章：跟这个人同处
    let near = jot(&mut store, work, "第二章的一个念头", vec![format!("chapter:{second}")]);

    let got = store.spark_collisions(work, None, 3).unwrap();
    let with_card: Vec<_> = got
        .iter()
        .filter(|c| {
            c.left.key == format!("entity:{card}") || c.right.key == format!("entity:{card}")
        })
        .collect();
    assert_eq!(with_card.len(), 1, "一张卡只上一场：{got:?}");
    let pair = with_card[0];
    assert_eq!(pair.relation.as_str(), "together", "出场表把'第二章'摊成了它的锚点");
    let spark_key = format!("spark:{near}");
    assert!(pair.left.key == spark_key || pair.right.key == spark_key, "搭上的是第二章那条");
}

#[test]
fn two_elements_never_pair_up() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    plan(&store, first, "这一章要摊牌");
    foreshadow(&mut store, work, "那把刀还没交代来历", first);
    person(&mut store, work, "李三", &[first]);

    assert!(pairs(&store, work).is_empty(), "三头都是元素，一对都不该有");
}

#[test]
fn saving_an_element_pair_writes_one_card_with_two_sources() {
    let (_dir, mut store) = fresh();
    let (work, _first, second) = book(&mut store);
    plan(&store, second, "这一章要摊牌");
    let spark = jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{second}")]);

    let receipt = store
        .save_collision(
            &format!("spark:{spark}"),
            &format!("plan:{second}"),
            "摊牌那一场正好接上雨夜",
            None,
            "test",
        )
        .unwrap();
    let saved = store.fragment(receipt.spark_id).unwrap();
    assert_eq!(saved.body, "摊牌那一场正好接上雨夜");
    assert_eq!(saved.kind, FragmentKind::Idea, "碰撞卡就是一张普通的灵感卡");
    assert_eq!(saved.derived_from, Some(spark), "derived_from 只指碎片那一头");
    assert_eq!(
        source_keys(&store, receipt.spark_id),
        vec![format!("spark:{spark}"), format!("plan:{second}")],
        "两头都记着（章纲那一条也是溯源）"
    );
    // 原来那条碎片一个字不动
    assert_eq!(store.fragment(spark).unwrap().body, "雨夜那一场要收线");
    // 撞过的那一对不再出现（新写下的那张卡也不跟这处章纲再撞回去）
    let taken = format!("spark:{spark}×plan:{second}");
    assert!(
        !pairs(&store, work).contains(&taken),
        "撞过的组合不该再冒头：{:?}",
        pairs(&store, work)
    );
}

#[test]
fn a_collided_pair_never_comes_back_but_other_pairs_still_do() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    let _a = jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    let _b = jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    let _c = jot(&mut store, work, "丙", vec![format!("chapter:{second}")]);
    let _d = jot(&mut store, work, "丁", vec![format!("chapter:{second}")]);

    let before = pairs(&store, work);
    assert_eq!(before.len(), 2, "四条的池子够两对：{before:?}");
    let (taken, kept) = (before[0].clone(), before[1].clone());
    let (left, right) = taken.split_once('×').unwrap();
    let receipt = store.save_collision(left, right, "撞出来的一句", None, "test").unwrap();

    let after = pairs(&store, work);
    assert!(!after.contains(&taken), "撞过的那对不再出现：{after:?}");
    assert!(after.contains(&kept), "另外那一对照旧：{after:?}");
    // 新写下的那张卡**不跟自己的源头撞回去**（那是把同一件事再想一遍）
    let mine = format!("spark:{}", receipt.spark_id);
    assert!(
        !after.iter().any(|p| p.split('×').any(|side| side == mine)),
        "撞出来的卡不该立刻跟源头成对：{after:?}"
    );
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
    let (left, right) = before[0].split_once('×').unwrap();
    let after = store.skip_collision(work, left, right, None, "test").unwrap();
    assert_eq!(after.len(), 1, "跳过一对之后剩下一对");
    assert!(pairs(&store, work).iter().all(|p| *p != before[0]), "下一次挑对还认这个记号");

    // 跳过**不进"撞过"那本账**：撤销之后它原样回来
    store.unskip_collision(work, left, right, None, "test").unwrap();
    assert_eq!(pairs(&store, work), before, "撤销跳过就该回到原来那一屏");
}

#[test]
fn skipping_a_pair_with_an_element_is_remembered_by_its_kind() {
    let (_dir, mut store) = fresh();
    let (work, _first, second) = book(&mut store);
    plan(&store, second, "这一章要摊牌");
    jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{second}")]);

    let before = pairs(&store, work);
    assert_eq!(before.len(), 1);
    let (left, right) = before[0].split_once('×').unwrap();
    assert!(store.skip_collision(work, left, right, None, "test").unwrap().is_empty());
    assert!(pairs(&store, work).is_empty(), "跳过之后它不该再冒头");
    assert_eq!(
        store.unskip_collision(work, left, right, None, "test").unwrap().len(),
        1,
        "撤销回来"
    );
    assert_eq!(pairs(&store, work), before);
}

#[test]
fn a_gone_element_is_reported_as_gone() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let spark = jot(&mut store, work, "一个念头", vec![format!("chapter:{first}")]);

    // 章纲被清空了
    plan(&store, first, "");
    assert_eq!(
        store
            .save_collision(&format!("spark:{spark}"), &format!("plan:{first}"), "撞一句", None, "test")
            .unwrap_err()
            .code(),
        codes::COLLISION_SIDE_NOT_FOUND
    );
    // 伏笔收了
    let line = foreshadow(&mut store, work, "那条线", first);
    store
        .conn()
        .execute("UPDATE foreshadows SET state = 'collected' WHERE id = ?1", [line])
        .unwrap();
    assert_eq!(
        store
            .save_collision(
                &format!("spark:{spark}"),
                &format!("foreshadow:{line}"),
                "撞一句",
                None,
                "test"
            )
            .unwrap_err()
            .code(),
        codes::COLLISION_SIDE_NOT_FOUND
    );
}

#[test]
fn empty_self_element_only_and_unknown_sides_are_rejected() {
    let (_dir, mut store) = fresh();
    let (work, first, second) = book(&mut store);
    let a = jot(&mut store, work, "甲", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "乙", vec![format!("chapter:{first}")]);
    plan(&store, second, "这一章要摊牌");
    let line = foreshadow(&mut store, work, "那条线", first);
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap().id;
    let stranger = jot(&mut store, other, "别人的", vec![]);

    let ka = format!("spark:{a}");
    let kb = format!("spark:{b}");
    assert_eq!(
        store.save_collision(&ka, &kb, "   ", None, "test").unwrap_err().code(),
        codes::IDEA_BODY_EMPTY
    );
    assert_eq!(
        store.save_collision(&ka, &ka, "自己跟自己", None, "test").unwrap_err().code(),
        codes::COLLISION_SAME_SIDE
    );
    // 元素 × 元素：挑对那一层不摆，保存这条路也不许
    assert_eq!(
        store
            .save_collision(
                &format!("plan:{second}"),
                &format!("foreshadow:{line}"),
                "两头都是元素",
                None,
                "test"
            )
            .unwrap_err()
            .code(),
        codes::COLLISION_NEEDS_SPARK
    );
    // 跨书
    assert_eq!(
        store
            .save_collision(&ka, &format!("spark:{stranger}"), "跨书", None, "test")
            .unwrap_err()
            .code(),
        codes::COLLISION_FOREIGN
    );
    // 认不出的写法 / 不存在的碎片
    assert_eq!(
        store.save_collision("mood:3", &kb, "怪写法", None, "test").unwrap_err().code(),
        codes::COLLISION_SIDE_UNKNOWN
    );
    assert_eq!(
        store.save_collision(&ka, "spark:9999", "不存在的", None, "test").unwrap_err().code(),
        codes::SPARK_NOT_FOUND
    );
}

#[test]
fn a_collision_card_joins_the_pool_like_any_other_spark() {
    let (_dir, mut store) = fresh();
    let (work, first, _second) = book(&mut store);
    let a = jot(&mut store, work, "雨夜那一场要收线", vec![format!("chapter:{first}")]);
    let b = jot(&mut store, work, "主角其实五年前就知道", vec![format!("chapter:{first}")]);
    let receipt = store
        .save_collision(
            &format!("spark:{a}"),
            &format!("spark:{b}"),
            "他知道真相那么久，那场雨夜就变味了",
            None,
            "test",
        )
        .unwrap();

    // 它跟别的灵感一样进候选池，而且**继承了那两条的关联锚点**（跟当前章的关联度照算）
    let candidates = store.spark_candidates(work, Some(first), 10).unwrap();
    let saved = candidates
        .iter()
        .find(|s| s.id == receipt.spark_id)
        .expect("撞出来的那张卡也在池子里");
    assert!(saved.anchors.contains(&format!("chapter:{first}")));
    assert_eq!(saved.gravity.anchor_hits, 1, "继承了源头那一条章锚点");
    // 它自己也能被采纳（走的是同一条路）
    let adopted = store.adopt_spark(receipt.spark_id, "event", None, "", "test").unwrap();
    assert!(adopted.event_id.is_some());
}
