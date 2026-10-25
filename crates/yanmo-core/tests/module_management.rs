//! 管理页数据验收：**卡片字段、排序、收藏/置顶偏好、冻结反映**。
//!
//! 这里不测卸载/冻结本身（各有自己的验收）；只测管理页看到的那一份。

use serde_json::json;
use yanmo_core::store::Store;
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

const A: &str = "app.example.alpha";
const B: &str = "app.example.beta";
const C: &str = "app.example.gamma";

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn manifest(id: &str, name: &str) -> ModuleManifest {
    let value = json!({
        "id": id,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": name },
        "description": { "zh-Hans": "一句话。" },
        "author": { "name": "Example Studio" },
        "version": "1.2.3",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": { "read": ["works"], "own_data": true }
    });
    ModuleManifest::from_json(&value.to_string()).unwrap()
}

fn grant() -> ModuleCapabilities {
    ModuleCapabilities {
        read: vec![CoreDataArea::Works],
        own_data: true,
        ..ModuleCapabilities::none()
    }
}

fn install(store: &mut Store, id: &str, name: &str) {
    store.install_module(&manifest(id, name), &grant()).unwrap();
}

#[test]
fn cards_show_what_the_management_page_needs() {
    let (_dir, mut store) = fresh();
    install(&mut store, A, "阿尔法");
    let cards = store.module_cards().unwrap();
    assert_eq!(cards.len(), 1);
    let card = &cards[0];
    assert_eq!(card.id, A);
    assert_eq!(card.name, "阿尔法");
    assert_eq!(card.description, "一句话。");
    assert_eq!(card.version, "1.2.3");
    assert_eq!(card.author, "Example Studio");
    assert!(card.enabled, "装了默认激活");
    assert!(!card.certified, "没认证的照实说");
    assert!(!card.favorite && !card.pinned);
    assert_eq!(card.reads, vec!["works".to_string()]);
    assert!(card.own_data);
    assert!(!card.network);
}

#[test]
fn background_is_a_user_preference_not_autostart() {
    let (_dir, mut store) = fresh();
    install(&mut store, A, "甲");
    assert!(!store.module_card(A).unwrap().background, "默认离开就停");
    store.set_module_background(A, true).unwrap();
    assert!(store.module_card(A).unwrap().background);
    assert!(store.module_card(A).unwrap().enabled, "后台偏好不改变激活状态");
    store.set_module_background(A, false).unwrap();
    assert!(!store.module_card(A).unwrap().background);
    assert!(store.set_module_background("app.example.none", true).is_err());
}

#[test]
fn pinned_comes_first_then_favorite_then_install_order() {
    let (_dir, mut store) = fresh();
    install(&mut store, A, "甲");
    install(&mut store, B, "乙");
    install(&mut store, C, "丙");

    store.set_module_favorite(B, true).unwrap();
    let ids: Vec<_> = store.module_cards().unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![B, A, C], "收藏的排前面，其余按安装顺序");

    store.set_module_pinned(C, true).unwrap();
    let ids: Vec<_> = store.module_cards().unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![C, B, A], "置顶压过收藏");

    store.set_module_pinned(C, false).unwrap();
    store.set_module_favorite(B, false).unwrap();
    let ids: Vec<_> = store.module_cards().unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![A, B, C], "撤销后回到安装顺序");
}

#[test]
fn freezing_is_visible_on_the_card() {
    let (_dir, mut store) = fresh();
    install(&mut store, A, "甲");
    store.set_module_enabled(A, false).unwrap();
    let card = &store.module_cards().unwrap()[0];
    assert!(!card.enabled, "冻结要看得见");
}

#[test]
fn preferences_are_logged_and_unknown_modules_are_rejected() {
    let (_dir, mut store) = fresh();
    install(&mut store, A, "甲");
    store.set_module_favorite(A, true).unwrap();
    store.set_module_pinned(A, true).unwrap();

    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity = 'modules' ORDER BY seq")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let prefers: Vec<_> = rows.iter().filter(|(op, _)| op == "prefer").collect();
    assert_eq!(prefers.len(), 2);
    assert!(prefers[1].1.contains("\"favorite\":true"));
    drop(stmt);

    assert_eq!(
        store.set_module_favorite("app.example.none", true).unwrap_err().code(),
        yanmo_core::error_codes::codes::MODULE_NOT_FOUND
    );
}

#[test]
fn network_is_a_grant_and_requires_a_declaration() {
    let (_dir, mut store) = fresh();
    install(&mut store, A, "甲");
    assert!(!store.module_card(A).unwrap().network, "默认不联网");
    let error = store.set_module_network(A, true).unwrap_err();
    assert_eq!(error.code(), yanmo_core::error_codes::codes::MODULE_GRANT_NOT_DECLARED);

    let value = json!({
        "id": B,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "乙" },
        "description": { "zh-Hans": "一句话。" },
        "author": { "name": "Example Studio" },
        "version": "1.2.3",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": { "read": [], "network": true }
    });
    let manifest = ModuleManifest::from_json(&value.to_string()).unwrap();
    store
        .install_module(
            &manifest,
            &ModuleCapabilities { network: false, ..ModuleCapabilities::none() },
        )
        .unwrap();
    store.set_module_network(B, true).unwrap();
    assert!(store.module_card(B).unwrap().network);
    store.set_module_network(B, false).unwrap();
    assert!(!store.module_card(B).unwrap().network);
}
