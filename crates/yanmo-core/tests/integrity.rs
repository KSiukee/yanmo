//! 库体检的**结论**：三态，别把"没查成"当成"坏了"。
//!
//! 来龙去脉见 [`yanmo_core::db::Integrity`] 的注释：库文件只读时，`PRAGMA quick_check`
//! 里的 FTS5 索引核对需要写权限，SQLite 会回一句 "unable to validate … readonly database"。
//! 原来的调用方拿"不是 ok"当"库有问题"，救援工具就对着一本**完好**的书喊
//! 「库文件有问题，先别做别的操作，赶紧导出留底找开发者」——把"没查成"说成"坏了"。

use yanmo_core::db::{self, Integrity};
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

#[test]
fn a_healthy_library_checks_out_clean() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let verdict = db::integrity(store.conn()).unwrap();
    assert_eq!(verdict, Integrity::Clean);
    assert_eq!(verdict.as_str(), "clean");
    assert_eq!(verdict.raw(), "ok");
}

#[test]
fn a_read_only_library_is_not_checked_not_broken() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let work = store.create_work(WorkKind::Novel, "只读体检").unwrap();
    let node = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    let body = "只读之前写下的字。";
    store.write_body(node, body).unwrap();

    // `PRAGMA query_only` 是"写不进去"的**可移植**造法：Windows 上要加只读属性、
    // Linux 上要改文件模式，用 SQLite 自己的开关最省事，语义也一样（写操作一律被拒）。
    store.conn().pragma_update(None, "query_only", true).unwrap();

    let verdict = db::integrity(store.conn()).unwrap();
    match &verdict {
        Integrity::NotChecked(text) => assert!(
            text.to_lowercase().contains("readonly database"),
            "没查成的原因应当是只读：{text}"
        ),
        other => panic!("只读库的结论应当是「没查成」，得到的是 {other:?}"),
    }
    assert_eq!(verdict.as_str(), "not_checked");
    // 原话照样留档：日志与报告要能贴出 SQLite 说了什么
    assert!(verdict.raw().contains("unable to validate"), "{}", verdict.raw());
    // 而"字还在不在"这件事不受影响：只读库照样读得出来
    assert_eq!(store.read_body(node).unwrap(), body, "只读不等于读不了");
}
