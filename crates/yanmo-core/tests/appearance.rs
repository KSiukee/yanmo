//! 外观偏好验收：**全局打底 + 每书可选覆盖**，只存改过的项，坏记录当没设过。
//!
//! 四条判据：
//! 1. 没设过 → 默认值（默认只有一处，读的时候才落）；
//! 2. 写进去的只留改过的项（稀疏），全改回"没设过"就把键清掉；
//! 3. 书的覆盖 **只覆盖它真设过的项**，其余继承全局；
//! 4. 坏 JSON 不报错、当没设过——界面不该被一条坏记录卡住。

use yanmo_core::model::WorkKind;
use yanmo_core::store::{Appearance, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 库里某个键的原文（没这个键就是 None）。
fn raw(store: &Store, key: &str) -> Option<String> {
    store
        .conn()
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

#[test]
fn defaults_apply_when_nothing_was_ever_set() {
    let (_dir, store) = fresh();
    let prefs = store.appearance(None).unwrap();
    assert!(prefs.jump_to_end_on_latest, "默认开");
    assert!(raw(&store, "appearance").is_none(), "读默认不该顺手写库");
}

#[test]
fn writes_stay_sparse_and_clear_when_back_to_default() {
    let (_dir, mut store) = fresh();
    let off = Appearance { jump_to_end_on_latest: Some(false) };
    store.set_appearance(None, &off).unwrap();

    assert!(!store.appearance(None).unwrap().jump_to_end_on_latest);
    let stored = raw(&store, "appearance").expect("改过就该有记录");
    assert!(stored.contains("jump_to_end_on_latest"));
    assert!(!stored.contains("null"), "只存改过的项，不写空档：{stored}");

    // 空 patch 是"这项不改"（稀疏语义）：键还在
    store.set_appearance(None, &Appearance::default()).unwrap();
    assert!(raw(&store, "appearance").is_some(), "空 patch 不该动已有记录");

    // 真的要"回到默认"要用 reset：键被清掉，读出来又是默认值
    store.reset_appearance(None).unwrap();
    assert!(raw(&store, "appearance").is_none(), "回到默认就该把键删掉");
    assert!(store.appearance(None).unwrap().jump_to_end_on_latest, "回到默认");
}

#[test]
fn a_books_override_only_covers_what_it_really_set() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    // 全局关掉；这本书单独打开
    store.set_appearance(None, &Appearance { jump_to_end_on_latest: Some(false) }).unwrap();
    store
        .set_appearance(Some(work.id), &Appearance { jump_to_end_on_latest: Some(true) })
        .unwrap();

    assert!(store.appearance(Some(work.id)).unwrap().jump_to_end_on_latest, "这本书的覆盖盖住全局");
    assert!(!store.appearance(None).unwrap().jump_to_end_on_latest, "全局没被动过");
    assert!(raw(&store, &format!("work.{}.appearance", work.id)).is_some(), "覆盖按书写键");

    // 另一本书没单独设过：继承全局（关）
    let other = store.create_work(WorkKind::Novel, "别的书").unwrap();
    assert!(!store.appearance(Some(other.id)).unwrap().jump_to_end_on_latest, "没设过就继承全局");
}

#[test]
fn a_broken_record_falls_back_to_defaults_instead_of_failing() {
    let (_dir, mut store) = fresh();
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('appearance', '这不是 JSON', 0)",
            [],
        )
        .unwrap();

    let prefs = store.appearance(None).unwrap();
    assert!(prefs.jump_to_end_on_latest, "读不出来当没设过：宁可回默认也不卡住界面");

    // 坏记录还能被正常覆盖写掉
    store
        .set_appearance(None, &Appearance { jump_to_end_on_latest: Some(false) })
        .unwrap();
    assert!(!store.appearance(None).unwrap().jump_to_end_on_latest);
}
