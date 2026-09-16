//! 大纲冲突检测验收：**四条规则 + 只报告 + 忽略能被记住**。
//!
//! 分两半：
//! - **规则那一半**是纯逻辑（喂值、看发现）——不碰库，跑得飞快，判据也写得死；
//! - **扫描那一半**把库里的设定卡与场景卡拉齐，交给规则（顺带验软删的不再报）。
//!
//! 判据都盯着"这件事对不对"，不写死具体文案：句子在界面字典里（核心零文案）。

use std::collections::BTreeMap;

use yanmo_core::model::{
    Attribute, EntityKind, NewEntityCard, NodeKind, SceneFields, WorkKind,
};
use yanmo_core::outline::{scan, IssueRule, OutlineData};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn card(id: i64, name: &str, aliases: &[&str], attrs: &[(&str, &str)]) -> yanmo_core::model::EntityCard {
    yanmo_core::model::EntityCard {
        id,
        work_id: 1,
        kind: EntityKind::Person,
        name: name.to_string(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        attributes: attrs
            .iter()
            .map(|(k, v)| Attribute { key: k.to_string(), value: v.to_string() })
            .collect(),
        note: String::new(),
        created_at: 0,
        updated_at: 0,
    }
}

fn scene(node_id: i64, title: &str, pov: &str, goal: &str, conflict: &str, outcome: &str) -> (i64, String, SceneFields) {
    (
        node_id,
        title.to_string(),
        SceneFields {
            node_id,
            pov: pov.to_string(),
            goal: goal.to_string(),
            conflict: conflict.to_string(),
            outcome: outcome.to_string(),
        },
    )
}

/// 只跑规则那一半（不碰库）。
fn rules(cards: &[yanmo_core::model::EntityCard], scenes: &[(i64, String, SceneFields)]) -> Vec<yanmo_core::outline::OutlineIssue> {
    scan(&OutlineData { cards, scenes })
}

fn param(issue: &yanmo_core::outline::OutlineIssue, key: &str) -> String {
    issue.params.get(key).cloned().unwrap_or_default()
}

#[test]
fn a_shared_name_or_alias_is_reported_once_with_both_sides() {
    let cards = vec![
        card(1, "陆文", &["阿文"], &[]),
        // 别称撞上另一个人的正名：一样糟（喊一声"阿文"两个人回头）
        card(2, "林昭", &["阿文"], &[]),
        card(3, "沈砚", &[], &[]),
    ];
    let found = rules(&cards, &[]);
    assert_eq!(found.len(), 1, "只报那一条撞车的：{found:?}");
    let issue = &found[0];
    assert_eq!(issue.rule, IssueRule::EntityNameClash);
    assert_eq!(issue.anchors, vec!["entity:1", "entity:2"], "两边的锚点都给（点得动）");
    assert_eq!(param(issue, "name"), "阿文");
    assert_eq!(param(issue, "count"), "2");
    assert_eq!(param(issue, "names"), "陆文,林昭", "名单用中性分隔；顿号由界面按字典拼");
}

#[test]
fn the_same_name_registered_twice_on_one_card_is_its_own_rule() {
    // 正名与别称是同一个字（手滑），或者别称里写了两遍
    let cards = vec![card(7, "陆文", &["陆文", "陆文"], &[])];
    let found = rules(&cards, &[]);
    assert!(
        found.iter().all(|issue| issue.rule == IssueRule::EntityNameRepeated),
        "同卡重复不算撞车（那是一张卡的事）：{found:?}"
    );
    assert!(!found.is_empty());
    assert_eq!(found[0].anchors, vec!["entity:7"]);
    assert_eq!(param(&found[0], "name"), "陆文");
}

#[test]
fn two_different_values_for_the_same_attribute_are_a_conflict_but_a_repeat_is_not() {
    let cards = vec![
        card(1, "陆文", &[], &[("发色", "黑"), ("发色", "白")]),
        card(2, "林昭", &[], &[("发色", "黑"), ("发色", "黑")]),
        // 空值不算一种说法：那是"还没填"
        card(3, "沈砚", &[], &[("佩剑", ""), ("佩剑", "青霜")]),
    ];
    let found = rules(&cards, &[]);
    assert_eq!(found.len(), 1, "只有那张真的两种说法的才报：{found:?}");
    let issue = &found[0];
    assert_eq!(issue.rule, IssueRule::EntityAttributeConflict);
    assert_eq!(issue.anchors, vec!["entity:1"]);
    assert_eq!(param(issue, "name"), "陆文");
    assert_eq!(param(issue, "key"), "发色");
    assert!(param(issue, "values").contains('黑') && param(issue, "values").contains('白'));
}

#[test]
fn a_scene_reports_exactly_which_fields_are_missing_and_goes_quiet_when_full() {
    let scenes = vec![
        scene(1, "开场", "林昭", "", "  \n ", ""),
        scene(2, "对峙", "陆文", "问出真相", "他不肯说", "翻了脸"),
        scene(3, "", "", "", "", ""),
    ];
    let found = rules(&[], &scenes);
    assert_eq!(found.len(), 2, "填全的那一场不该报：{found:?}");
    let issue = &found[0];
    assert_eq!(issue.rule, IssueRule::SceneMissingFields);
    assert_eq!(issue.anchors, vec!["scene:1"]);
    assert_eq!(param(issue, "title"), "开场");
    // 只有空白也算没填：goal / conflict / outcome 三格都空着
    assert_eq!(param(issue, "count"), "3");
    assert_eq!(param(issue, "missing"), "goal,conflict,outcome", "缺哪几格说清楚（稳定码）");
    // 没起名的那些也照报（名字由界面说"这一张还没起名"）
    assert_eq!(param(&found[1], "count"), "4");
}

#[test]
fn the_same_data_scans_to_the_same_list_and_fingerprints_are_stable() {
    let cards = vec![card(1, "陆文", &["阿文"], &[("发色", "黑"), ("发色", "白")]), card(2, "林昭", &["阿文"], &[])];
    let scenes = vec![scene(9, "开场", "林昭", "", "", "")];
    let first = rules(&cards, &scenes);
    let second = rules(&cards, &scenes);
    assert_eq!(first, second, "两次扫描结果一模一样（界面上的清单不该自己跳）");
    assert!(first.len() >= 3, "三类都在：{first:?}");

    // 身份只认主体：**填了一格，忽略过的那条还是同一条**
    let before = first
        .iter()
        .find(|issue| issue.rule == IssueRule::SceneMissingFields)
        .unwrap()
        .fingerprint();
    let after = rules(&cards, &[scene(9, "开场", "林昭", "拿到账本", "", "")]);
    let after = after
        .iter()
        .find(|issue| issue.rule == IssueRule::SceneMissingFields)
        .unwrap()
        .fingerprint();
    assert_eq!(before, after, "状态变了，身份不该变（否则忽略会被自己的编辑打断）");
}

#[test]
fn scanning_the_library_reports_every_rule_and_respects_soft_deletes() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();
    let scene = store.create_node(work.id, Some(chapter), NodeKind::Scene, "开场").unwrap();

    let mut first = NewEntityCard {
        work_id: work.id,
        kind: EntityKind::Person,
        name: "陆文".to_string(),
        aliases: vec!["阿文".to_string()],
        attributes: vec![
            Attribute { key: "发色".to_string(), value: "黑".to_string() },
            Attribute { key: "发色".to_string(), value: "白".to_string() },
        ],
        note: String::new(),
    };
    let first_id = store.create_entity_card(&first, "test").unwrap();
    first.name = "林昭".to_string();
    first.attributes.clear();
    store.create_entity_card(&first, "test").unwrap();

    let found = store.outline_issues(work.id).unwrap();
    let rules_found: Vec<IssueRule> = found.iter().map(|issue| issue.rule).collect();
    assert!(rules_found.contains(&IssueRule::EntityNameClash), "{found:?}");
    assert!(rules_found.contains(&IssueRule::EntityAttributeConflict), "{found:?}");
    assert!(rules_found.contains(&IssueRule::SceneMissingFields), "{found:?}");

    // 软删一张卡：撞车那条跟着消失（不再列它）
    store.delete_entity_card(first_id, "test").unwrap();
    let after = store.outline_issues(work.id).unwrap();
    assert!(
        !after.iter().any(|issue| issue.rule == IssueRule::EntityAttributeConflict),
        "删掉的那张卡不该再报属性冲突：{after:?}"
    );
    assert!(
        !after.iter().any(|issue| issue.rule == IssueRule::EntityNameClash),
        "撞车的另一边没了，这条也不该再报：{after:?}"
    );

    // 场景卡填全：缺项那条消失
    store
        .save_scene_fields(
            &SceneFields {
                node_id: scene,
                pov: "陆文".to_string(),
                goal: "拿到账本".to_string(),
                conflict: "他不肯给".to_string(),
                outcome: "抢到了".to_string(),
            },
            "test",
        )
        .unwrap();
    let filled = store.outline_issues(work.id).unwrap();
    assert!(!filled.iter().any(|issue| issue.rule == IssueRule::SceneMissingFields));
}

#[test]
fn dismissing_is_remembered_idempotent_and_has_a_way_back() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let key = "scene.missing_fields|scene:9|";

    assert!(store.dismissed_issues(work.id).unwrap().is_empty());
    assert_eq!(store.dismiss_issue(work.id, key, "test").unwrap(), vec![key]);
    // 连点两下不算错，也不会记两条
    assert_eq!(store.dismiss_issue(work.id, key, "test").unwrap(), vec![key]);
    assert_eq!(store.dismissed_issues(work.id).unwrap(), vec![key]);

    // 按书分：另一本书没被牵连
    let other = store.create_work(WorkKind::Novel, "白日").unwrap();
    assert!(store.dismissed_issues(other.id).unwrap().is_empty());

    // 回头路：撤销一条 / 全部重新看
    store.undismiss_issue(work.id, key, "test").unwrap();
    assert!(store.dismissed_issues(work.id).unwrap().is_empty());
    store.dismiss_issue(work.id, key, "test").unwrap();
    store.clear_dismissed_issues(work.id, "test").unwrap();
    assert!(store.dismissed_issues(work.id).unwrap().is_empty());
}

#[test]
fn a_broken_dismissal_record_reads_as_nothing_dismissed() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, '不是 JSON', 0)",
            [format!("work.{}.outline.dismissed", work.id)],
        )
        .unwrap();
    // 读不出来就当没记过（界面会重新报一遍——那比"清单空了"强）
    assert!(store.dismissed_issues(work.id).unwrap().is_empty());
}

/// 参数表是 `BTreeMap`，"两个值"的顺序也稳定（界面上那行不该一闪一个样）。
#[test]
fn the_values_param_keeps_a_stable_order() {
    let cards = vec![card(1, "陆文", &[], &[("发色", "白"), ("发色", "黑")])];
    let found = rules(&cards, &[]);
    let values: &BTreeMap<String, String> = &found[0].params;
    assert_eq!(values.get("values").map(String::as_str), Some("白 / 黑"), "按值排，不按录入顺序");
}
