//! 故意弄错：**垃圾输入与边界值只允许明确报错，绝不允许 panic**。
//!
//! 这一层很重要：壳里的命令一旦 panic，赔上的是整个进程（作者手上的未落盘内容一起没）。
//! 所以每条边界都要有测试盯着——**报错可以接受，崩不行**。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 坏查询词：FTS5 的 MATCH 有自己的一套语法，用户随手打的字符不该把它弄崩。
const NASTY_QUERIES: &[&str] = &[
    "", "   ", "\"", "\"\"", "*", "**", "(", ")", "NEAR(", "a OR b", "NOT x", "^-", "\"未闭合",
    "``", "{}", "\\", "%%", "__", "\u{0}", "\u{200b}", "😀😀😀", "1e999", "SELECT * FROM nodes",
];

#[test]
fn garbage_queries_never_panic_and_never_surprise() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "边界测试").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(node, "正文里有 引号\" 与 星号* 还有 NEAR( 这种词。").unwrap();

    for query in NASTY_QUERIES {
        // 只要求"不 panic、返回 Result"；空查询应当返回空结果
        let hits = store.search(query, None, 10).expect("坏查询不应让检索失败");
        if query.trim().is_empty() {
            assert!(hits.is_empty(), "空查询不该有命中：{query:?}");
        }
    }
}

#[test]
fn out_of_range_ids_are_errors_not_crashes() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "边界测试").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;

    for bad in [-1i64, 0, i64::MIN, i64::MAX] {
        assert!(store.write_body(bad, "内容").is_err(), "写不存在的节点应当报错：{bad}");
        assert!(store.rename_node(bad, "改名").is_err());
        assert!(store.soft_delete_node(bad).is_err());
        assert!(store.node_title(bad).is_err());
        assert!(store.snapshot_if_changed(bad, "probe").is_err());
        // 读一个不存在的节点是"空"，不是错误（界面会读到空章）
        assert_eq!(store.read_body(bad).unwrap(), "");
        assert_eq!(store.body_fingerprint(bad).unwrap(), "");
    }
    // 合法节点仍然正常
    assert_eq!(store.node_title(node).unwrap(), "边界测试");
}

#[test]
fn weird_titles_and_bodies_are_handled() {
    let (_dir, mut store) = fresh();

    // 超长标题（10 万字）：允许成功，也允许明确拒绝，但不许崩
    let huge_title = "长".repeat(100_000);
    let created = store.create_work(WorkKind::Article, &huge_title);
    if let Ok(work) = created {
        let node = store.list_nodes(work.id).unwrap()[0].id;
        assert_eq!(store.node_title(node).unwrap().chars().count(), 100_000);
    }

    // 只有空白 / 零宽字符 / emoji 的标题
    //
    // 规矩：**纯空白 = 还没起名**（允许，界面按语言补占位）；零宽字符与 emoji 原样留着——
    // 我们不替作者判断什么才算"好名字"，只把首尾空白去掉。
    for title in ["   ", "\u{200b}", "😀", "\t\n"] {
        let work = store.create_work(WorkKind::Article, title).expect("这些标题都该被接受");
        assert!(
            !work.title.contains(char::is_whitespace),
            "首尾空白要去掉：{:?}",
            work.title
        );
    }
    assert_eq!(
        store.create_work(WorkKind::Article, "   ").unwrap().title,
        "",
        "纯空白标题要落成空串，不能落成别的什么默认名"
    );

    // 正文里塞进各种怪东西
    let work = store.create_work(WorkKind::Article, "怪正文").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;
    let weird = "零宽\u{200b}字符\r\n回车制表\t结束😀\u{0}控制符";
    let stats = store.write_body(node, weird).unwrap();
    assert_eq!(store.read_body(node).unwrap(), weird);
    assert!(stats.char_count > 0 && stats.word_count > 0);
}

#[test]
fn title_from_another_work_cannot_be_smuggled_in() {
    let (_dir, mut store) = fresh();
    let a = store.create_work(WorkKind::Novel, "甲").unwrap();
    let b = store.create_work(WorkKind::Novel, "乙").unwrap();
    let a_root = store.list_nodes(a.id).unwrap()[0].id;
    let b_root = store.list_nodes(b.id).unwrap()[0].id;

    // 跨作品挂载、把根节点挂到自己子孙下：都必须报错
    assert!(store.create_node(a.id, Some(b_root), NodeKind::Chapter, "越界").is_err());
    assert!(store.move_node(a_root, Some(a_root), 0).is_err());
    assert!(store.move_node(a_root, Some(b_root), 0).is_err());
}

#[test]
fn file_names_stay_legal_for_hostile_titles() {
    for title in ["", "///", "..", "con", "a\u{0}b", "😀".repeat(200).as_str(), "\u{200b}"] {
        let name = yanmo_core::atomic::safe_file_name(title);
        assert!(!name.is_empty(), "文件名不能为空：{title:?}");
        assert!(!name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\u{0}']));
        assert!(name.chars().count() <= 60);
        assert!(!name.ends_with('.') && !name.ends_with(' '));
    }
}
