//! 关窗 / 崩溃恢复的验收：**已落盘的一个字都不能少**，以及"上次是不是正常退出"必须判得准。
//!
//! 这里模拟"杀进程"的方式很直接：**写完不调用 `end_session` 就把存储句柄丢掉**——
//! 与任务管理器结束进程对数据库而言是同一件事（连接没了，标记还写着"正在运行"）。

use std::time::Duration;

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    (dir, path)
}

/// 读会话标记（内部结构，测试里直接看一眼就够了）。
fn marker(store: &Store) -> serde_json::Value {
    let raw: String = store
        .conn()
        .query_row("SELECT value FROM settings WHERE key = 'session.last'", [], |r| r.get(0))
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn snapshot_reasons(store: &Store, node_id: i64) -> Vec<String> {
    let mut stmt = store
        .conn()
        .prepare("SELECT reason FROM snapshots WHERE node_id = ?1 ORDER BY id")
        .unwrap();
    let rows = stmt.query_map(rusqlite::params![node_id], |r| r.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn killed_process_is_detected_and_nothing_written_is_lost() {
    let (_dir, path) = fresh();
    let node_id;
    {
        let mut store = Store::open(&path).unwrap();
        let report = store.begin_session().unwrap();
        assert!(!report.unclean, "全新库不该报告崩溃");

        let target = store.ensure_editor_target().unwrap();
        node_id = target.node_id;
        store.write_body(node_id, "写到一半，然后进程被杀。").unwrap();
        // ★ 故意不 end_session —— 这就是"杀进程"
    }
    {
        let mut store = Store::open(&path).unwrap();
        let report = store.begin_session().unwrap();
        assert!(report.unclean, "上一个标记还是「正在运行」→ 必须判定为未正常退出");
        assert_eq!(report.last_node_id, Some(node_id), "要能说出崩溃前编辑的是哪一章");
        assert!(report.last_seen_at.unwrap_or(0) > 0);

        assert_eq!(
            store.read_body(node_id).unwrap(),
            "写到一半，然后进程被杀。",
            "已落盘的一个字都不能少"
        );
        assert_eq!(
            store.search("进程被杀", None, 10).unwrap().len(),
            1,
            "检索索引也应当完好（触发器跟着写入走）"
        );
    }
}

#[test]
fn graceful_exit_is_clean_and_leaves_a_close_snapshot() {
    let (_dir, path) = fresh();
    let node_id;
    {
        let mut store = Store::open(&path).unwrap();
        store.begin_session().unwrap();
        let target = store.ensure_editor_target().unwrap();
        node_id = target.node_id;
        store.write_body(node_id, "正常退出前的最后一版。").unwrap();

        assert!(store.end_session(node_id).unwrap(), "关窗时应当留下快照");
        assert_eq!(snapshot_reasons(&store, node_id), vec!["close"]);
    }
    {
        let mut store = Store::open(&path).unwrap();
        let report = store.begin_session().unwrap();
        assert!(!report.unclean, "正常退出后再开不该报崩溃");
        assert_eq!(store.read_body(node_id).unwrap(), "正常退出前的最后一版。");
    }
}

#[test]
fn close_snapshot_is_idempotent() {
    let (_dir, path) = fresh();
    let mut store = Store::open(&path).unwrap();
    store.begin_session().unwrap();
    let node_id = store.ensure_editor_target().unwrap().node_id;
    store.write_body(node_id, "同一版内容。").unwrap();

    assert!(store.end_session(node_id).unwrap(), "第一次关窗要留快照");
    assert!(!store.end_session(node_id).unwrap(), "内容没变就不该再堆一条");
    assert_eq!(snapshot_reasons(&store, node_id), vec!["close"], "快照表不该被关窗动作刷爆");

    // 内容变了才会再留一条
    store.write_body(node_id, "改了一版内容。").unwrap();
    assert!(store.end_session(node_id).unwrap());
    assert_eq!(snapshot_reasons(&store, node_id), vec!["close", "close"]);
}

#[test]
fn abandon_session_marks_clean_without_snapshot() {
    let (_dir, path) = fresh();
    let mut store = Store::open(&path).unwrap();
    store.begin_session().unwrap();
    let node_id = store.ensure_editor_target().unwrap().node_id;
    store.write_body(node_id, "用户选择仍然退出。").unwrap();

    store.abandon_session().unwrap();
    assert!(snapshot_reasons(&store, node_id).is_empty(), "放弃会话不该写快照");
    drop(store);

    let mut store = Store::open(&path).unwrap();
    assert!(!store.begin_session().unwrap().unclean, "用户主动放弃＝正常退出");
}

#[test]
fn heartbeat_advances_on_saves_and_readback_checks() {
    let (_dir, path) = fresh();
    let mut store = Store::open(&path).unwrap();
    store.begin_session().unwrap();
    let node_id = store.ensure_editor_target().unwrap().node_id;

    store.write_body(node_id, "第一版").unwrap();
    let after_save = marker(&store);
    assert_eq!(after_save["node_id"], serde_json::json!(node_id), "落盘要顺带记住是哪一章");
    assert_eq!(after_save["clean"], serde_json::json!(false));

    std::thread::sleep(Duration::from_millis(5));
    store.body_fingerprint(node_id).unwrap(); // 界面每几秒的读回校验＝心跳
    let after_check = marker(&store);
    assert!(
        after_check["heartbeat_at"].as_i64() > after_save["heartbeat_at"].as_i64(),
        "读回校验应当把心跳往前推：{after_save} → {after_check}"
    );
}

#[test]
fn a_read_only_check_after_a_clean_close_does_not_report_a_crash() {
    // 2026-09-15 代码质量评审：中等 11。关窗时 `end_session` 标"干净退出"，而界面还能再轮询
    // 一次读回校验——那次"读"以前会把 clean 标回 false，于是下次启动误报"上次没有正常退出"。
    let (_dir, path) = fresh();
    let node_id;
    {
        let mut store = Store::open(&path).unwrap();
        store.begin_session().unwrap();
        node_id = store.ensure_editor_target().unwrap().node_id;
        store.write_body(node_id, "写完就关。").unwrap();
        store.end_session(node_id).unwrap(); // 正常退出：标干净
        // 关窗之后界面可能还来得及再校验一次（这正是误报的来源）
        store.body_fingerprint(node_id).unwrap();
    }

    let mut store = Store::open(&path).unwrap();
    let report = store.begin_session().unwrap();
    assert!(!report.unclean, "一次只读的读回校验不该把干净退出标成崩溃");
}

#[test]
fn crash_recovery_reopens_the_chapter_that_was_being_edited() {
    let (_dir, path) = fresh();
    let second;
    {
        let mut store = Store::open(&path).unwrap();
        store.begin_session().unwrap();
        let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
        let volume = store.list_nodes(work.id).unwrap()[0].id;
        let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();
        second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第二章").unwrap();
        store.write_body(first, "第一章的正文。").unwrap();
        store.write_body(second, "第二章的正文。").unwrap();
        // 又被杀了
    }

    let mut store = Store::open(&path).unwrap();
    let report = store.begin_session().unwrap();
    assert!(report.unclean);
    let target = store.ensure_editor_target_preferring(report.last_node_id).unwrap();
    assert_eq!(target.node_id, second, "重开应当回到崩溃前那一章");
    assert_eq!(target.title, "第二章");
    assert_eq!(store.read_body(target.node_id).unwrap(), "第二章的正文。");

    // 那一章要是被删了，就老实回到默认引导，而不是硬凑
    store.soft_delete_node(second).unwrap();
    let fallback = store.ensure_editor_target_preferring(Some(second)).unwrap();
    assert_ne!(fallback.node_id, second);
    assert!(store.read_body(fallback.node_id).unwrap().contains("第一章"));
}

#[test]
fn opened_chapter_is_remembered_even_before_any_save() {
    let (_dir, path) = fresh();
    let opened;
    {
        let mut store = Store::open(&path).unwrap();
        store.begin_session().unwrap();
        opened = store.ensure_editor_target().unwrap().node_id;
        store.note_open_node(opened).unwrap();
        // 一个字都还没写就被杀
    }

    let mut store = Store::open(&path).unwrap();
    let report = store.begin_session().unwrap();
    assert!(report.unclean);
    assert_eq!(
        report.last_node_id,
        Some(opened),
        "还没落盘就被杀，也要能说出当时开的是哪一章"
    );
}

// ── 守护之心：界面卡死的证据 ──────────────────────────────────────────────
//
// 这一组守的是"卡过这件事**留得下、说得清、不会反复念**"：
//   ① 判死那一刻，现场（第几次、卡在哪一章、最后一版的指纹）进会话标记，同时进 op-log；
//   ② 正常退出把它清掉——它是给"这一次重载"看的，不该下次开窗还念；
//   ③ 真被杀了（没有正常退出）时它**必须还在**：那正是作者最需要知道的一次。

/// op-log 里的卡死记录（新的在前）。
fn freeze_log(store: &Store) -> Vec<(String, serde_json::Value)> {
    let mut stmt = store
        .conn()
        .prepare("SELECT op, payload FROM op_log WHERE entity = 'ui' ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap();
    rows.map(|row| {
        let (op, payload) = row.unwrap();
        (op, serde_json::from_str(&payload).unwrap())
    })
    .collect()
}

#[test]
fn a_freeze_leaves_its_scene_in_the_marker_and_the_op_log() {
    let (_dir, path) = fresh();
    let mut store = Store::open(&path).unwrap();
    store.begin_session().unwrap();
    let node = store.ensure_editor_target().unwrap().node_id;
    store.write_body(node, "卡死之前写下的这一版。").unwrap();
    // 落盘那一路顺带把"最后一版的指纹"记进标记（卡死证据直接抄它）
    let written = marker(&store)["fingerprint"].as_str().unwrap().to_string();
    assert!(!written.is_empty(), "落盘之后标记里该有指纹了");

    assert!(store.peek_session().unwrap().revive.is_none(), "没卡过就不该有交代");

    store.note_ui_freeze(1, false).unwrap();
    let report = store.peek_session().unwrap();
    let revive = report.revive.expect("卡过就要留交代");
    assert_eq!(revive.attempt, 1);
    assert!(!revive.gave_up, "第一次判死时还在试着重载");
    assert_eq!(revive.node_id, Some(node), "要记着卡在哪一章（重开回到原位靠它）");
    assert_eq!(revive.fingerprint, written, "最后一版的指纹也要记下");
    assert!(revive.at > 0);

    let log = freeze_log(&store);
    assert_eq!(log.len(), 1, "同一次事务里还要在 op-log 留一条");
    assert_eq!(log[0].0, "freeze");
    assert_eq!(log[0].1["attempt"], 1);
    assert_eq!(log[0].1["node_id"], node);

    // 停手时把同一条交代更新成最终结论，并再留一条（"我试过了、没成"）
    store.note_ui_freeze(3, true).unwrap();
    let revive = store.peek_session().unwrap().revive.unwrap();
    assert_eq!(revive.attempt, 3);
    assert!(revive.gave_up);
    let log = freeze_log(&store);
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].0, "freeze_gave_up");
}

#[test]
fn a_clean_exit_clears_the_freeze_note() {
    let (_dir, path) = fresh();
    let mut store = Store::open(&path).unwrap();
    store.begin_session().unwrap();
    let node = store.ensure_editor_target().unwrap().node_id;
    store.note_ui_freeze(1, false).unwrap();

    store.end_session(node).unwrap();
    assert!(
        store.peek_session().unwrap().revive.is_none(),
        "正常退出之后不该再念旧事——那一次重载时已经告诉过作者了"
    );

    // 再开一次也不会翻出来
    let mut again = Store::open(&path).unwrap();
    let report = again.begin_session().unwrap();
    assert!(!report.unclean);
    assert!(report.revive.is_none());
}

#[test]
fn a_freeze_survives_a_kill_and_is_reported_with_the_crash() {
    let (_dir, path) = fresh();
    let node;
    {
        let mut store = Store::open(&path).unwrap();
        store.begin_session().unwrap();
        node = store.ensure_editor_target().unwrap().node_id;
        store.write_body(node, "刚卡死就被杀了。").unwrap();
        store.note_ui_freeze(2, true).unwrap();
        // ★ 故意不 end_session —— 这就是"卡死之后被杀"
    }
    {
        let mut store = Store::open(&path).unwrap();
        let report = store.begin_session().unwrap();
        assert!(report.unclean, "被杀这件事照旧要报");
        let revive = report.revive.expect("卡死那次也该留下");
        assert_eq!(revive.attempt, 2);
        assert!(revive.gave_up, "停手了就得说清");
        assert_eq!(revive.node_id, Some(node));
        // 而且：正文一个字没少（证据只是证据，不许影响稿子）
        let (body, _) = store.read_body_with_stats(node).unwrap();
        assert_eq!(body, "刚卡死就被杀了。");
    }
}
