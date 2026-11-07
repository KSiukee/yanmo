//! 每本书的笔名：**只覆盖笔名这一栏**，真名/电话/通联仍跟人走；清掉就回全局。
//!
//! 投稿出口（`author_block_for`）与界面读口（`author_for`）必须看到同一份。

use yanmo_core::model::{Author, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn a_work_pen_name_overrides_only_the_pen_name() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let other = store.create_work(WorkKind::Novel, "别的书").unwrap();

    store
        .set_author(&Author {
            pen_name: Some("全局笔名".to_string()),
            real_name: Some("真名".to_string()),
            phone: Some("13800000000".to_string()),
            attach: Some(true),
            ..Default::default()
        })
        .unwrap();

    // 没设过 → 跟全局
    assert_eq!(store.author_for(work.id).unwrap().pen_name, "全局笔名");
    assert!(store.pen_name_for(work.id).unwrap().is_none());

    // 设了 → 只有这一本换，别的书照旧
    assert_eq!(
        store.set_pen_name_for(work.id, Some("本书笔名")).unwrap().as_deref(),
        Some("本书笔名")
    );
    let resolved = store.author_for(work.id).unwrap();
    assert_eq!(resolved.pen_name, "本书笔名", "笔名换成这一本的");
    assert_eq!(resolved.real_name, "真名", "真名仍跟人走");
    assert_eq!(resolved.phone, "13800000000", "电话仍跟人走");
    assert_eq!(store.author_for(other.id).unwrap().pen_name, "全局笔名", "别的书不受影响");

    // 投稿出口看到的是同一份
    let block = store.author_block_for(work.id).unwrap().expect("勾了附简介就该有");
    assert_eq!(block.pen_name, "本书笔名");
    assert_eq!(block.real_name, "真名");

    // 清掉 → 回全局
    store.set_pen_name_for(work.id, None).unwrap();
    assert_eq!(store.author_for(work.id).unwrap().pen_name, "全局笔名");
    assert!(store.pen_name_for(work.id).unwrap().is_none());
}

#[test]
fn an_empty_work_pen_name_means_follow_the_global_one() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    store
        .set_author(&Author { pen_name: Some("全局笔名".to_string()), ..Default::default() })
        .unwrap();

    assert!(store.set_pen_name_for(work.id, Some("   ")).unwrap().is_none());
    assert_eq!(store.author_for(work.id).unwrap().pen_name, "全局笔名");

    // 过长当场拒，不留半条记录
    let too_long = "长".repeat(201);
    assert!(store.set_pen_name_for(work.id, Some(&too_long)).is_err());
    assert!(store.pen_name_for(work.id).unwrap().is_none());
}
