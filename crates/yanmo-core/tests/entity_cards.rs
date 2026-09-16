//! 设定卡（人物 / 设定）验收：**名字是身份、属性是键值、删是软删**。
//!
//! 这一份盯五件事：
//! 1. 建/改/列/删一条龙对得上（整卡覆盖：改完读回来就是那一份）；
//! 2. **名字不能空**（修剪后）——它是身份，冲突检测就认它；
//! 3. 空项入口就丢（别称、整条空的属性），但"只有键没值"的那条**留着**（那是还没填）；
//! 4. 软删：删完列不到、取不到，行还在；
//! 5. 坏 JSON 如实报错（要拿去做冲突检测的列，不许静默少一截）。

use yanmo_core::error::codes;
use yanmo_core::model::{Attribute, EntityKind, NewEntityCard, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn card(work_id: i64, kind: EntityKind, name: &str) -> NewEntityCard {
    NewEntityCard {
        work_id,
        kind,
        name: name.to_string(),
        aliases: Vec::new(),
        attributes: Vec::new(),
        note: String::new(),
    }
}

#[test]
fn a_card_round_trips_and_updates_whole_card() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    let mut draft = card(work.id, EntityKind::Person, "陆文");
    draft.aliases = vec!["阿文".to_string(), " 陆大人 ".to_string()];
    draft.attributes = vec![
        Attribute { key: "发色".to_string(), value: "黑".to_string() },
        Attribute { key: " 年龄 ".to_string(), value: " 二十七 ".to_string() },
    ];
    draft.note = "  主角，前朝旧臣  ".to_string();
    let id = store.create_entity_card(&draft, "test").unwrap();

    let back = store.entity_card(id).unwrap();
    assert_eq!(back.name, "陆文");
    assert_eq!(back.kind, EntityKind::Person);
    assert_eq!(back.aliases, vec!["阿文", "陆大人"], "别称修剪了首尾空白");
    assert_eq!(back.attributes[1].key, "年龄", "属性键值也修剪");
    assert_eq!(back.attributes[1].value, "二十七");
    assert_eq!(back.note, "主角，前朝旧臣");

    // 整卡覆盖：改成什么样，读回来就是什么样（没传的项也照它覆盖）
    let mut changed = card(work.id, EntityKind::Setting, "藏书阁");
    changed.aliases = vec!["旧阁".to_string()];
    store.update_entity_card(id, &changed, "test").unwrap();
    let after = store.entity_card(id).unwrap();
    assert_eq!(after.name, "藏书阁");
    assert_eq!(after.kind, EntityKind::Setting);
    assert_eq!(after.aliases, vec!["旧阁"]);
    assert!(after.attributes.is_empty(), "整卡覆盖：上一版的属性没带过来");
    assert_eq!(after.note, "");
}

#[test]
fn an_empty_name_is_refused_and_nothing_is_written() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let err = store
        .create_entity_card(&card(work.id, EntityKind::Person, "   "), "test")
        .unwrap_err();
    assert_eq!(err.code(), codes::ENTITY_NAME_EMPTY);

    let count: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM entity_cards", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "被拒的建卡不许留下半行");
}

#[test]
fn blank_aliases_and_blank_attribute_rows_are_dropped_but_kept_when_only_the_value_is_missing() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    let mut draft = card(work.id, EntityKind::Person, "陆文");
    draft.aliases = vec!["阿文".to_string(), "   ".to_string(), String::new()];
    draft.attributes = vec![
        Attribute { key: "发色".to_string(), value: "黑".to_string() },
        // 整条空的：手滑，丢掉
        Attribute { key: "  ".to_string(), value: "  ".to_string() },
        // 只有键没有值：**留着**——那是"还没填"，不是手滑
        Attribute { key: "佩剑".to_string(), value: String::new() },
    ];
    let id = store.create_entity_card(&draft, "test").unwrap();

    let back = store.entity_card(id).unwrap();
    assert_eq!(back.aliases, vec!["阿文"], "空的别称不留");
    assert_eq!(back.attributes.len(), 2, "整条空的丢掉，只有键的那条留着：{:?}", back.attributes);
    let kept = back.attributes.iter().find(|a| a.key == "佩剑").expect("只有键的那条要在");
    assert_eq!(kept.value, "");
}

#[test]
fn listing_is_per_work_and_can_be_filtered_by_kind() {
    let (_dir, mut store) = fresh();
    let one = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let two = store.create_work(WorkKind::Novel, "白日").unwrap();

    store.create_entity_card(&card(one.id, EntityKind::Setting, "藏书阁"), "test").unwrap();
    store.create_entity_card(&card(one.id, EntityKind::Person, "陆文"), "test").unwrap();
    store.create_entity_card(&card(two.id, EntityKind::Person, "别人"), "test").unwrap();

    let all = store.entity_cards(one.id, None).unwrap();
    assert_eq!(all.len(), 2, "只列这本书的");
    assert_eq!(all[0].name, "藏书阁", "按名字排");
    let persons = store.entity_cards(one.id, Some(EntityKind::Person)).unwrap();
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].name, "陆文");
}

#[test]
fn deleting_is_soft_and_the_row_stays() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_entity_card(&card(work.id, EntityKind::Person, "陆文"), "test")
        .unwrap();

    let gone = store.delete_entity_card(id, "test").unwrap();
    assert_eq!(gone.name, "陆文", "回执给的是它删掉的那一张（调用方要靠它回一屏）");
    assert!(store.entity_card(id).is_err(), "删掉的按取一张就是不存在");
    assert!(store.entity_cards(work.id, None).unwrap().is_empty());
    let rows: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM entity_cards WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "软删：行还在，只是打了时间戳");
}

#[test]
fn a_broken_json_column_is_reported_instead_of_read_as_empty() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_entity_card(&card(work.id, EntityKind::Person, "陆文"), "test")
        .unwrap();

    store
        .conn()
        .execute("UPDATE entity_cards SET attributes='不是 JSON' WHERE id=?1", [id])
        .unwrap();
    let err = store.entity_card(id).unwrap_err();
    assert_eq!(err.code(), codes::ENTITY_BAD_FIELD);
    // 列出来的时候同样如实报（不许静默当空：那等于少报一处冲突）
    assert!(store.entity_cards(work.id, None).is_err());
}

#[test]
fn the_kind_is_a_closed_set() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let id = store
        .create_entity_card(&card(work.id, EntityKind::Person, "陆文"), "test")
        .unwrap();
    store
        .conn()
        .execute("UPDATE entity_cards SET card_kind='creature' WHERE id=?1", [id])
        .unwrap();
    assert_eq!(
        store.entity_card(id).unwrap_err().code(),
        codes::UNKNOWN_ENTITY_KIND,
        "库里认不出的类型如实报错，不猜一个像那么回事的"
    );
}
