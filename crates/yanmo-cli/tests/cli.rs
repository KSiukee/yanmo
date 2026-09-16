//! 命令行入口的验收：**这条入口要能替脚本把整件事干完**。
//!
//! 四条判据：
//! 1. 建书 → 建章 → 写正文 → 读回来，一条龙对得上；
//! 2. 「上次没正常退出」判得出来，而且**只读的看一眼不会把状态盖掉**；
//! 3. 体检对健康的库给出明确结论（完整 + 结构版本）；
//! 4. 写错的命令 / 选项**当场报用法错误**，绝不静默忽略。

use std::path::Path;

use serde_json::Value;
use yanmo_cli::args::Args;
use yanmo_cli::{execute, CliError};

fn run(dir: &Path, command: &str, options: &[(&str, &str)]) -> Result<Value, CliError> {
    let args = Args {
        data: dir.to_path_buf(),
        command: command.to_string(),
        options: options.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect(),
    };
    execute(&args)
}

fn ok(dir: &Path, command: &str, options: &[(&str, &str)]) -> Value {
    let value = run(dir, command, options).unwrap_or_else(|error| panic!("{command} 应当成功：{error:?}"));
    assert_eq!(value["ok"], true, "{command} 应当成功：{value}");
    value
}

/// 建一本书 + 一章，返回 `(work_id, node_id)`。
fn seed(dir: &Path, kind: &str) -> (i64, i64) {
    let work = ok(dir, "new-work", &[("kind", kind), ("title", "长夜")]);
    let work_id = work["work_id"].as_i64().expect("新建的书要有 id");
    let chapter = ok(
        dir,
        "new-node",
        &[("work", &work_id.to_string()), ("kind", "chapter"), ("title", "第一章")],
    );
    let node_id = chapter["node_id"].as_i64().expect("新建的节点要有 id");
    (work_id, node_id)
}

#[test]
fn book_chapter_text_and_tree_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, node_id) = seed(dir.path(), "novel");

    let text = "第一段。\n\n第二段。";
    let written = ok(dir.path(), "write", &[("node", &node_id.to_string()), ("body", text)]);
    assert_eq!(written["word_count"].as_i64().unwrap(), 6, "中文按字算：第一段 + 第二段");
    let fingerprint = written["fingerprint"].as_str().unwrap().to_string();

    let read = ok(dir.path(), "read", &[("node", &node_id.to_string())]);
    assert_eq!(read["body"], text, "写进去什么，读出来就是什么");

    let again = ok(dir.path(), "fingerprint", &[("node", &node_id.to_string())]);
    assert_eq!(again["fingerprint"].as_str().unwrap(), fingerprint);

    let nodes = ok(dir.path(), "nodes", &[("work", &work_id.to_string())]);
    let list = nodes["nodes"].as_array().unwrap();
    assert!(
        list.iter().any(|node| node["id"] == node_id && node["title"] == "第一章" && node["has_body"] == true),
        "目录里要看得见这一章与它的正文：{nodes}"
    );
}

#[test]
fn a_session_that_never_ended_shows_up_as_unclean() {
    let dir = tempfile::tempdir().unwrap();
    let (_work_id, node_id) = seed(dir.path(), "novel");

    // 先走一遍完整流程（启动 → 正常收尾）：库里留下的标记是「上次退干净了」
    ok(dir.path(), "begin", &[]);
    ok(dir.path(), "end", &[("node", &node_id.to_string())]);

    // 只看一眼：**不许**把「上次退干净了」这个结论改掉（看两次还是干净的）
    let peeked = ok(dir.path(), "report", &[]);
    assert_eq!(peeked["session"]["unclean"], false, "正常收尾过就该是干净的");
    let peeked_again = ok(dir.path(), "report", &[]);
    assert_eq!(peeked_again["session"]["unclean"], false, "只看一眼不该改变任何状态");

    // 启动之后没正常收尾（= 被杀）：下一次启动必须报出来
    ok(dir.path(), "begin", &[]);
    let dirty = ok(dir.path(), "begin", &[]);
    assert_eq!(dirty["session"]["unclean"], true, "没正常收尾就该报出来");

    // 记下正在写哪一章，再正常收尾：下一次启动干净，且记得上次在哪一章
    ok(dir.path(), "note-open", &[("node", &node_id.to_string())]);
    ok(dir.path(), "end", &[("node", &node_id.to_string())]);
    let third = ok(dir.path(), "begin", &[]);
    assert_eq!(third["session"]["unclean"], false, "正常收尾之后应当干净");
    assert_eq!(
        third["session"]["last_node_id"].as_i64(),
        Some(node_id),
        "标记里要留着上次在写哪一章"
    );
}

#[test]
fn works_lists_the_shelf() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, _node_id) = seed(dir.path(), "novel");

    let works = ok(dir.path(), "works", &[]);
    let list = works["works"].as_array().unwrap();
    assert!(
        list.iter().any(|work| work["id"] == work_id && work["title"] == "长夜"),
        "书架上要看得见这本书：{works}"
    );
}

#[test]
fn verify_reports_a_healthy_database() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "novel");

    let report = ok(dir.path(), "verify", &[]);
    assert_eq!(report["integrity"], "ok");
    assert_eq!(
        report["schema_version"].as_i64().unwrap(),
        i64::from(yanmo_core::db::migrations::schema_version()),
        "结构版本应当跟到最新一次迁移"
    );
    assert!(!report["engine_version"].as_str().unwrap().is_empty());
}

#[test]
fn export_writes_the_book_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, node_id) = seed(dir.path(), "article");
    ok(dir.path(), "write", &[("node", &node_id.to_string()), ("body", "短歌行。")]);

    let out = dir.path().join("out");
    let exported = ok(
        dir.path(),
        "export",
        &[("work", &work_id.to_string()), ("format", "txt"), ("out", out.to_str().unwrap())],
    );
    assert!(exported["files"].as_i64().unwrap() >= 1, "{exported}");
    assert!(std::fs::read_dir(&out).unwrap().count() >= 1, "导出目录里应当真有文件");
}

#[test]
fn search_finds_text_and_titles() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, node_id) = seed(dir.path(), "novel");
    ok(
        dir.path(),
        "write",
        &[("node", &node_id.to_string()), ("body", "他把那枚铜钱按在桌上，指节发白。")],
    );

    // 长短语走全文索引：命中正文，片段要带着那串字
    let long = ok(dir.path(), "search", &[("query", "指节发白")]);
    let hits = long["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "应命中一章：{long}");
    assert_eq!(hits[0]["node_id"], node_id);
    assert_eq!(hits[0]["work_id"], work_id);
    assert!(hits[0]["snippet"].as_str().unwrap().contains("指节发白"), "{long}");

    // 两个字太短，全文索引表示不了：回退全扫也得搜得到（中文里这是常态）
    let short = ok(dir.path(), "search", &[("query", "铜钱")]);
    assert_eq!(short["count"].as_i64(), Some(1), "两字查询也要搜得到：{short}");

    // 命中标题时片段是空串，但 matched_title 要说清楚
    let titled = ok(dir.path(), "search", &[("query", "第一章")]);
    let titled_hits = titled["hits"].as_array().unwrap();
    assert_eq!(titled_hits[0]["matched_title"], true, "{titled}");
    assert_eq!(titled_hits[0]["snippet"].as_str().unwrap(), "", "只在标题里命中时正文片段是空的");

    // 搜不到就是空结果，不是报错
    let none = ok(dir.path(), "search", &[("query", "这句话书里没有")]);
    assert_eq!(none["count"].as_i64(), Some(0), "{none}");

    // --work 限定到别的书：结果是空的
    let other = ok(dir.path(), "new-work", &[("kind", "novel"), ("title", "另一本")]);
    let other_id = other["work_id"].as_i64().unwrap();
    let scoped = ok(dir.path(), "search", &[("query", "指节发白"), ("work", &other_id.to_string())]);
    assert_eq!(scoped["count"].as_i64(), Some(0), "限定到别的书就不该有命中：{scoped}");
}

#[test]
fn search_needs_a_query_and_rejects_bad_options() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "novel");
    assert!(
        matches!(run(dir.path(), "search", &[]), Err(CliError::Usage(_))),
        "不给 --query 要报用法错误"
    );
    assert!(
        matches!(run(dir.path(), "search", &[("query", "字"), ("work", "不是数字")]), Err(CliError::Usage(_))),
        "--work 不是整数要报用法错误"
    );
    assert!(
        matches!(run(dir.path(), "search", &[("query", "字"), ("node", "1")]), Err(CliError::Usage(_))),
        "search 不认 --node，写错要报出来"
    );
}

#[test]
fn wrong_command_or_option_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(run(dir.path(), "dance", &[]), Err(CliError::Usage(_))), "不认识的命令要报错");
    assert!(
        matches!(run(dir.path(), "begin", &[("nope", "1")]), Err(CliError::Usage(_))),
        "不认识的选项要报错"
    );
    // 把 --node 写成 --nodes：**必须报错**，不能被静默忽略（否则用例会"静默用错章"）
    assert!(
        matches!(run(dir.path(), "read", &[("nodes", "1")]), Err(CliError::Usage(_))),
        "选项名写错要当场报出来"
    );
    // read 只认 --node：拿 --work 来读正文属于用错命令，要报错而不是默默读错东西
    assert!(
        matches!(run(dir.path(), "read", &[("work", "1")]), Err(CliError::Usage(_))),
        "命令与选项对不上要报出来"
    );
}

#[test]
fn import_only_writes_when_asked_and_then_the_book_is_there() {
    let dir = tempfile::tempdir().unwrap();
    // 先造一份成稿（用开发档命令建书 → 导出 json），再从它导入
    let (work_id, node_id) = seed(dir.path(), "novel");
    ok(dir.path(), "write", &[("node", &node_id.to_string()), ("body", "雨下了整夜。")]);
    let source = dir.path().join("成稿.json");
    let exported = ok(
        dir.path(),
        "export",
        &[("work", &work_id.to_string()), ("format", "json"), ("out", dir.path().to_str().unwrap())],
    );
    assert_eq!(exported["ok"], true);
    std::fs::copy(dir.path().join("work.json"), &source).unwrap();
    std::fs::remove_file(dir.path().join("work.json")).unwrap();

    // 另一本"新库"（模拟库没了）：里面先放一本作者自己的书
    let target = tempfile::tempdir().unwrap();
    let (mine, my_chapter) = seed(target.path(), "novel");
    ok(target.path(), "write", &[("node", &my_chapter.to_string()), ("body", "我自己写的。")]);
    let before = ok(target.path(), "works", &[]);

    // 干跑：只算不写
    let dry = ok(target.path(), "import", &[("from", source.to_str().unwrap())]);
    assert_eq!(dry["wrote"], false);
    assert_eq!(dry["count"], 1);
    assert_eq!(dry["drafts"][0]["title"], "长夜");
    assert_eq!(dry["drafts"][0]["scale"]["word_count"], 5, "雨下了整夜 → 五个字");
    assert_eq!(dry["drafts"][0]["same_title_in_library"], 1, "新库里已经有一本同名的");
    assert_eq!(ok(target.path(), "works", &[])["works"].as_array().unwrap().len(), 1, "干跑不许动库");

    // 真写：只新建，不动既有那本
    let written = ok(
        target.path(),
        "import",
        &[("from", source.to_str().unwrap()), ("work", "长夜"), ("yes", "")],
    );
    assert_eq!(written["wrote"], true);
    let new_id = written["drafts"][0]["imported"]["work_id"].as_i64().unwrap();
    assert_ne!(new_id, mine, "导入的是新的一本，不是覆盖既有那本");
    let after = ok(target.path(), "works", &[]);
    assert_eq!(after["works"].as_array().unwrap().len(), 2);
    assert_eq!(after["works"][0]["chapters"], before["works"][0]["chapters"], "既有那本一字未动");

    // 坏成稿：当场报错，且说得出是哪一格
    let broken = target.path().join("坏成稿.json");
    std::fs::write(&broken, "{\"title\":\"x\",\"kind\":\"novel\",\"nodes\":[{\"kind\":\"章\"}]}").unwrap();
    match run(target.path(), "import", &[("from", broken.to_str().unwrap())]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "value.unknown_node_kind"),
        other => panic!("坏成稿该被拒绝：{other:?}"),
    }
    // 找不到成稿：用法错误（不是"悄悄成功导了 0 本"）
    assert!(matches!(
        run(target.path(), "import", &[("from", target.path().to_str().unwrap())]),
        Err(CliError::Usage(_))
    ));
    assert!(matches!(run(target.path(), "import", &[]), Err(CliError::Usage(_))), "--from 是必填");
}

#[test]
fn a_broken_draft_in_the_batch_stops_the_whole_import_before_anything_is_written() {
    // 2026-09-15 代码质量评审：轻微 19——帮助文本承诺"一份成稿读不进来就整批停下（不许救一半）"，
    // 而老实现是边解析边写：第 2 份坏掉时第 1 份已经入库了。现在两遍法：先全解析，再统一写。
    //
    // 造一份真成稿（建书 → 导出 json）放进包里，再配一份坏成稿排在它后面
    // （`find_drafts` 用 BTreeSet 排序，A 开头的一定先被解析到）。
    let library = tempfile::tempdir().unwrap();
    let (work_id, node_id) = seed(library.path(), "novel");
    ok(library.path(), "write", &[("node", &node_id.to_string()), ("body", "甲书的正文。")]);
    ok(
        library.path(),
        "export",
        &[("work", &work_id.to_string()), ("format", "json"), ("out", library.path().to_str().unwrap())],
    );

    let pack = tempfile::tempdir().unwrap();
    let good = pack.path().join("成稿").join("A书");
    std::fs::create_dir_all(&good).unwrap();
    std::fs::copy(library.path().join("work.json"), good.join("work.json")).unwrap();
    let bad = pack.path().join("成稿").join("Z坏书");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("work.json"),
        r#"{"title":"Z坏书","kind":"novel","nodes":[{"kind":"章"}]}"#,
    )
    .unwrap();

    let target = tempfile::tempdir().unwrap();
    seed(target.path(), "novel"); // 库里先有作者自己的一本
    let before = ok(target.path(), "works", &[])["works"].as_array().unwrap().len();

    match run(target.path(), "import", &[("from", pack.path().to_str().unwrap()), ("yes", "")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "value.unknown_node_kind"),
        other => panic!("坏成稿该被拒绝：{other:?}"),
    }
    assert_eq!(
        ok(target.path(), "works", &[])["works"].as_array().unwrap().len(),
        before,
        "整批停下：好那一本也不许先进库"
    );
}

#[test]
fn a_counted_write_lands_in_the_daily_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let (_, node_id) = seed(dir.path(), "novel");

    // 不给 --tz：只写正文，不进账本（备份恢复、脚本灌数据走这条）
    ok(dir.path(), "write", &[("node", &node_id.to_string()), ("body", "不计账的字。")]);
    let plain = ledger_rows(dir.path());
    assert_eq!(plain, 0, "不给 --tz 就不该记进每日码字");

    // 给了 --tz：编辑器落盘那条路，按作者本地时区记账
    ok(
        dir.path(),
        "write",
        &[("node", &node_id.to_string()), ("body", "计账的字。"), ("tz", "480")],
    );
    assert_eq!(ledger_rows(dir.path()), 1, "给了 --tz 就该留下这一天的账");
    assert!(
        matches!(run(dir.path(), "write", &[("node", "1"), ("body", "x"), ("tz", "东八")]), Err(CliError::Usage(_))),
        "--tz 不是整数要报用法错误"
    );
}

/// 每日码字账本里有几行（直接读库：命令行没有"看账本"的命令，这是驱动方的活）。
fn ledger_rows(dir: &Path) -> i64 {
    let conn = yanmo_core::db::open_ready(dir.join("yanmo.db")).unwrap();
    conn.query_row("SELECT COUNT(*) FROM writing_days", [], |row| row.get(0)).unwrap()
}

#[test]
fn a_backup_from_the_command_line_writes_a_package_and_a_skipped_target_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, node_id) = seed(dir.path(), "novel");
    ok(dir.path(), "write", &[("node", &node_id.to_string()), ("body", "备份之前写下的字。")]);
    let target = dir.path().join("备份盘");

    // ① 正常备份：包里该有的东西都点了名，读回体检过
    let report = ok(
        dir.path(),
        "backup",
        &[("to", target.to_str().unwrap()), ("keep", "3"), ("device", "演练台")],
    );
    assert_eq!(report["succeeded"], 1, "{report}");
    let package = std::path::Path::new(report["outcomes"][0]["package"].as_str().unwrap());
    assert!(package.join("yanmo.db").is_file(), "快照要在包里");
    assert!(package.join("manifest.json").is_file(), "清单要在包里");

    // ② 目标不可达（拿一个"文件下面"的路径当目录）：**跳过并记账**，不算整体失败
    let blocked = dir.path().join("占位文件");
    std::fs::write(&blocked, b"x").unwrap();
    let bad = ok(
        dir.path(),
        "backup",
        &[("to", blocked.join("子目录").to_str().unwrap())],
    );
    assert_eq!(bad["succeeded"], 0, "{bad}");
    // 目标不可达算「跳过」（不是「失败」）：跳过与失败都记进账本，但语义不同——
    // 跳过是"这一处没成，别的目标照做"，失败是"做备份这件事本身出问题了"。
    assert_eq!(bad["skipped"], 1, "不可达的目标要记成跳过：{bad}");
    assert_eq!(bad["failed"], 0, "不可达不该算整体失败：{bad}");
    assert!(
        !bad["outcomes"][0]["reason"].as_str().unwrap_or("").is_empty(),
        "跳过也要给原因，别只说跳过：{bad}"
    );

    // ③ 账本里两次尝试都记着（成功与跳过都记，空档不用猜）
    let ledger: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("backup-ledger.json")).unwrap())
            .unwrap();
    let entries = ledger["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "两次尝试都该记进账本：{ledger}");
    assert_eq!(entries[0]["status"], "written");
    assert_eq!(entries[1]["status"], "skipped");
    assert!(!entries[1]["reason"].as_str().unwrap_or("").is_empty());
    assert!(work_id > 0);
}

#[test]
fn a_read_only_command_never_creates_an_empty_library() {
    // 2026-09-15 代码质量评审：轻微 16/26——`--data` 写错一个字符时，只读命令会当场造出一个空库、
    // 再一本正经地回"没有作品"，作者看到的正是"稿子没了"。现在只认已经存在的库。
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("这不是稿库");

    let error = run(&empty, "verify", &[]).expect_err("空目录里不该开出一个新库");
    let text = format!("{error:?}");
    assert!(text.contains("store.missing"), "要回机器可读的码（store.missing）：{text}");

    assert!(!empty.join("yanmo.db").exists(), "拒绝之后不许留下空库");
    assert!(!empty.exists(), "连目录都不该建出来");
}

#[test]
fn help_text_tells_the_truth_about_opening_the_library() {
    // 2026-09-15 代码质量评审：中等 4——看稿那几条走的是 `Store::open`，会跑结构迁移、
    // 登记本机、给老库回填字数标记。帮助里原来写着"只看不写（不会改动稿库）"，是句错话。
    // 这条守卫钉住"如实描述"，免得哪天又被改回一句好听但不真的承诺。
    let help = yanmo_cli::args::HELP_RESCUE;
    assert!(help.contains("会打开库"), "帮助必须说清它会打开库（可能升级结构）：\n{help}");
    assert!(!help.contains("不会改动稿库"), "不许再自称不改稿库（那是假的）：\n{help}");
    assert!(help.contains("先把库文件复制一份"), "要体检坏库得先提示复制一份");
}

/// 叩问问题卡：**命令行能把 6 态状态机从头驱动一遍**，每条迁移都有回执与可核对的历史。
///
/// 这条测试在验两件事：① 状态机在命令行上够得着（外部演练台不必等界面）；② 每次迁移
/// 留下的证据（`op` / `from` / `to` / `trigger`）能被独立读回来——"谁触发、从哪到哪"。
#[test]
fn question_card_states_are_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, _node) = seed(dir.path(), "novel");
    let work = work_id.to_string();

    let made = ok(
        dir.path(),
        "card-new",
        &[
            ("work", &work),
            ("body", "第 12 章埋下的信物，现在该让它露头了吗？"),
            ("template", "foreshadow.due"),
            ("importance", "0.8"),
        ],
    );
    let card_id = made["card_id"].as_i64().expect("建卡要给 id");
    let card = card_id.to_string();
    assert_eq!(
        ok(dir.path(), "card-list", &[("work", &work), ("state", "pending")])["count"],
        1,
        "新建的卡在「待问」上"
    );

    // pending → asked：回执说清从哪到哪，并记上新颖度的账
    let asked =
        ok(dir.path(), "card-move", &[("id", &card), ("to", "asked"), ("trigger", "push")]);
    assert_eq!(asked["from"], "pending");
    assert_eq!(asked["to"], "asked");
    assert_eq!(asked["used_count"], 1, "问出一次，账加一（新颖度冷却靠它）");

    // asked → answered：终态
    ok(dir.path(), "card-move", &[("id", &card), ("to", "answered"), ("trigger", "author")]);
    assert_eq!(ok(dir.path(), "card-list", &[("work", &work), ("state", "answered")])["count"], 1);

    // 终态出不去：非法边当场给码，不是静默不动
    match run(dir.path(), "card-move", &[("id", &card), ("to", "pending")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "card.illegal_transition"),
        other => panic!("answered 是终态，该出不去：{other:?}"),
    }

    // 第二条卡：延后 → 重出 → 舍弃 → 捞回 → 静音 → 解除，一条不落
    let second = ok(dir.path(), "card-new", &[("work", &work), ("body", "要不要插个喘息？")]);
    let second_id = second["card_id"].as_i64().unwrap();
    let second = second_id.to_string();
    for (to, trigger) in [
        ("asked", "pull"),
        ("deferred", "author"),
        ("pending", "requeue"),
        ("asked", "push"),
        ("discarded", "author"),
        ("pending", "retrieve"),
        ("muted", "author"),
        ("pending", "unmute"),
    ] {
        ok(dir.path(), "card-move", &[("id", &second), ("to", to), ("trigger", trigger)]);
    }
    assert_eq!(
        ok(dir.path(), "card-list", &[("work", &work), ("state", "pending")])["count"],
        1,
        "捞回 / 解除静音之后，这张卡又回到候选池里"
    );

    // 迁移史：谁触发、从哪到哪，一条不少、顺序不乱
    let events = ok(dir.path(), "card-events", &[("id", &second)]);
    let ops: Vec<&str> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["op"].as_str().unwrap())
        .collect();
    assert_eq!(
        ops,
        ["create", "ask", "defer", "requeue", "ask", "discard", "retrieve", "mute", "unmute"],
        "事件历史该按发生顺序摊开：{events}"
    );
    assert_eq!(events["events"][2]["from"], "asked", "延后是从「已问」出发的");
    assert_eq!(events["events"][2]["trigger"], "author");

    // 认不出来的态要给出码（不是用法错，也不是默默当成 pending）
    match run(dir.path(), "card-move", &[("id", &card), ("to", "nowhere")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "value.unknown_question_state"),
        other => panic!("不认识的态该被拒：{other:?}"),
    }
}

/// 叩问的选题与偏好：命令行能把「生成草稿 → 按引力选题 → 处置即学习 → 解除静音」走一遍。
///
/// 这条测试盯的是**机制能从外面驱动、且每一步都可核对**：草稿里只有模板与槽位（没有句子）、
/// 选题是只读的、同锚点不会重复问、说好只教同类模板、静音是整类开关且可撤销。
#[test]
fn question_selection_and_preference_are_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let anchor = format!("chapter:{chapter_id}");

    // ① 草稿：这一章还没动笔 → 问「从哪儿开始」，外加**开篇两问**（谁在看 / 第一场戏在哪儿）
    let drafts = ok(dir.path(), "question-draft", &[("work", &work)]);
    assert_eq!(drafts["count"], 3, "{drafts}");
    let keys: Vec<&str> = drafts["drafts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|draft| draft["template_key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["chapter.empty_body", "plan.opening_pov", "plan.opening_scene"]);
    for draft in drafts["drafts"].as_array().unwrap() {
        assert_eq!(draft["slots"]["chapter"], "第一章");
        assert_eq!(draft["anchors"][0], anchor, "开篇两问锚在开头那一章上");
    }

    // ② 落成卡（真实链路里句子由界面按语言渲染；这里给一句占位正文）
    let made = ok(
        dir.path(),
        "card-new",
        &[
            ("work", &work),
            ("body", "占位"),
            ("template", "chapter.empty_body"),
            ("importance", "0.7"),
            ("linked", &anchor),
        ],
    );
    let first = made["card_id"].as_i64().unwrap();
    let after = ok(dir.path(), "question-draft", &[("work", &work)]);
    assert_eq!(after["count"], 2, "同一个锚点上同一条模板，问过就不该再生成：{after}");
    assert!(
        !after["drafts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|draft| draft["template_key"] == "chapter.empty_body"),
        "问过的那一条不再生成"
    );

    // ③ 选题（只读）：候选里有它，且引力是**拆解过的**（将来界面能回答"为什么问这个"）
    let picked = ok(dir.path(), "question-select", &[("work", &work), ("limit", "3")]);
    assert_eq!(picked["questions"][0]["card_id"], first);
    assert_eq!(picked["questions"][0]["gravity"]["novelty"], 1.0, "没问过的新颖度是满的");
    assert_eq!(picked["questions"][0]["gravity"]["derived_discount"], 1.0);

    // ④ 说「这个问题好」：状态一个字节不动，只教同类模板
    let praised = ok(dir.path(), "question-praise", &[("id", &first.to_string()), ("trigger", "author")]);
    assert_eq!(praised["learned"]["positives"], 1);
    assert!(praised["learned"]["weight"].as_f64().unwrap() > 1.0);
    assert_eq!(
        ok(dir.path(), "card-list", &[("work", &work), ("state", "pending")])["count"],
        1,
        "评价与处置是两件事：夸过之后它还在待问上"
    );

    // ⑤ 静音那一类（第二张同模板的卡还待问着）→ 整类从候选里消失 → 解除静音又回来
    let second = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "第二条"), ("template", "chapter.empty_body"), ("linked", "chapter:99")],
    )["card_id"]
        .as_i64()
        .unwrap();
    ok(dir.path(), "card-move", &[("id", &first.to_string()), ("to", "muted"), ("trigger", "author")]);
    let weights = ok(dir.path(), "question-weights", &[]);
    assert_eq!(weights["weights"][0]["template_key"], "chapter.empty_body");
    assert_eq!(weights["weights"][0]["enabled"], false, "静音把这一类整体停用");
    assert_eq!(
        ok(dir.path(), "question-select", &[("work", &work)])["count"],
        0,
        "停用的一类不排到后面，是不出现（第二张卡虽然待问着）"
    );

    ok(dir.path(), "question-unmute", &[("template", "chapter.empty_body")]);
    let back = ok(dir.path(), "question-select", &[("work", &work)]);
    assert_eq!(back["count"], 1, "解除静音之后又回到候选池");
    assert_eq!(back["questions"][0]["card_id"], second);
}

/// 叩问的延后队列：命令行能把「带条件延后 → 查队列 → 到条件重出」走一遍。
///
/// 盯三件事：延后**离开了候选池**、条件**没到就不回来**、到了才回来并留下可核对的痕迹
/// （队列记录标掉 + 事件里写清是哪条条件到了）。
#[test]
fn question_deferral_queue_is_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let anchor = format!("chapter:{chapter_id}");

    let card = ok(
        dir.path(),
        "card-new",
        &[
            ("work", &work),
            ("body", "占位"),
            ("template", "chapter.empty_body"),
            ("linked", &anchor),
        ],
    )["card_id"]
        .as_i64()
        .unwrap();
    ok(dir.path(), "card-move", &[("id", &card.to_string()), ("to", "asked"), ("trigger", "push")]);

    // ① 按预置档延后：界面上作者点的是"什么时候再问我"，不是一堆参数
    let made = ok(
        dir.path(),
        "question-defer",
        &[
            ("id", &card.to_string()),
            ("preset", "when_chapter_written"),
            ("note", "等写到第二章再说"),
            ("trigger", "author"),
        ],
    );
    assert!(made["deferral_id"].as_i64().unwrap() > 0, "{made}");

    // ② 队列读得回来；延后之后它不在候选池里
    let queue = ok(dir.path(), "question-deferrals", &[("work", &work)]);
    assert_eq!(queue["count"], 1, "{queue}");
    assert_eq!(queue["deferrals"][0]["kind"], "written");
    assert_eq!(queue["deferrals"][0]["anchor_node"], chapter_id);
    assert_eq!(queue["deferrals"][0]["note"], "等写到第二章再说");
    assert_eq!(ok(dir.path(), "question-select", &[("work", &work)])["count"], 0);

    // ③ 那一章还没写 → 重出扫描什么都不做
    assert_eq!(
        ok(dir.path(), "question-requeue", &[("work", &work), ("trigger", "timer")])["count"],
        0
    );

    // ④ 写完之后再扫 → 回池子，并留痕（哪条条件到了写得一清二楚）
    ok(dir.path(), "write", &[("node", &chapter_id.to_string()), ("body", "第二章的正文。")]);
    let back = ok(dir.path(), "question-requeue", &[("work", &work), ("trigger", "timer")]);
    assert_eq!(back["count"], 1, "{back}");
    assert_eq!(back["cards"][0], card);
    assert_eq!(ok(dir.path(), "card-list", &[("work", &work), ("state", "pending")])["count"], 1);
    let events = ok(dir.path(), "card-events", &[("id", &card.to_string())]);
    let last = events["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["op"], "requeue");
    assert_eq!(last["trigger"], "timer:written");
    assert_eq!(
        ok(dir.path(), "question-deferrals", &[("work", &work)])["count"],
        0,
        "重出之后队列里不该还挂着它"
    );

    // ⑤ 时间档：给天数的写法也要能用；预置键写错报**用法错**（不是静默当成默认）
    let second = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "第二条"), ("template", "chapter.empty_body")],
    )["card_id"]
        .as_i64()
        .unwrap();
    ok(dir.path(), "card-move", &[("id", &second.to_string()), ("to", "asked"), ("trigger", "push")]);
    let timed = ok(
        dir.path(),
        "question-defer",
        &[("id", &second.to_string()), ("kind", "time"), ("after-days", "3")],
    );
    assert!(timed["deferral_id"].as_i64().unwrap() > 0);
    assert!(matches!(
        run(dir.path(), "question-defer", &[("id", &second.to_string()), ("preset", "someday")]),
        Err(CliError::Usage(_))
    ));
}

/// 叩问的处置四件套：命令行能把「舍弃进冷却库 → 捞回 → 静音来源 → 记灵感」走一遍。
///
/// 盯的是处置的**语义**：舍弃不等于删除（冷却库读得回来、能捞回、还成了负样本）、
/// 静音只让那个来源闭嘴（不是把叩问关掉）、记灵感**不动问题的状态**（正交）。
#[test]
fn question_disposition_is_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let anchor = format!("chapter:{chapter_id}");

    let ours = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "占位"), ("template", "chapter.empty_body"), ("linked", &anchor)],
    )["card_id"]
        .as_i64()
        .unwrap();
    let noisy = ok(
        dir.path(),
        "card-new",
        &[
            ("work", &work),
            ("body", "来自模块的一条"),
            ("template", "rhythm.length_swing"),
            ("source", "module-x"),
        ],
    )["card_id"]
        .as_i64()
        .unwrap();
    for id in [ours, noisy] {
        ok(dir.path(), "card-move", &[("id", &id.to_string()), ("to", "asked"), ("trigger", "push")]);
    }

    // 舍弃 → 冷却库；它就是这类问题的负样本（同类模板当场降权）
    ok(dir.path(), "card-move", &[("id", &noisy.to_string()), ("to", "discarded"), ("trigger", "author")]);
    let cooled = ok(dir.path(), "question-cooled", &[("work", &work)]);
    assert_eq!(cooled["count"], 1, "{cooled}");
    assert_eq!(cooled["cooled"][0]["card_id"], noisy);
    let weights = ok(dir.path(), "question-weights", &[]);
    let learned = weights["weights"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["template_key"] == "rhythm.length_swing")
        .expect("舍弃过的模板要留下学习记录");
    assert!(learned["weight"].as_f64().unwrap() < 1.0, "{learned}");

    // 捞回：回到池子，冷却库里没有它了
    ok(dir.path(), "question-retrieve", &[("id", &noisy.to_string())]);
    assert_eq!(ok(dir.path(), "question-cooled", &[("work", &work)])["count"], 0);

    // 按来源静音：只让那个模块闭嘴
    let muted = ok(dir.path(), "question-mute-source", &[("source", "module-x")]);
    assert_eq!(muted["muted"][0], "module-x");
    let picked = ok(dir.path(), "question-select", &[("work", &work)]);
    assert!(
        !picked["questions"].as_array().unwrap().iter().any(|q| q["card_id"] == noisy),
        "静音来源的问题不出现：{picked}"
    );
    assert_eq!(ok(dir.path(), "question-sources", &[])["count"], 1);
    assert_eq!(
        ok(dir.path(), "question-mute-source", &[("source", "module-x"), ("off", "")])["muted"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // 记灵感：落一张带溯源的灵感卡，**问题的状态一个字节不动**
    let idea = ok(
        dir.path(),
        "question-inspire",
        &[("id", &ours.to_string()), ("body", "让他把那封信烧了"), ("source", "typed")],
    );
    let idea_id = idea["idea_id"].as_i64().unwrap();
    let ideas = ok(dir.path(), "question-inspirations", &[("id", &ours.to_string())]);
    assert_eq!(ideas["count"], 1, "{ideas}");
    assert_eq!(ideas["inspirations"][0]["id"], idea_id);
    assert_eq!(ideas["inspirations"][0]["derived_from"], ours);
    assert_eq!(ideas["inspirations"][0]["body"], "让他把那封信烧了");
    assert_eq!(
        ok(dir.path(), "card-list", &[("work", &work), ("state", "asked")])["count"],
        1,
        "记灵感不改状态：这张卡还在「已问」上"
    );

    // 「这类别再问」与它的回头路：静音之后权重表里能看到停用，解除之后又启用
    ok(dir.path(), "card-move", &[("id", &ours.to_string()), ("to", "muted"), ("trigger", "author")]);
    let muting = ok(dir.path(), "question-weights", &[]);
    let row = muting["weights"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["template_key"] == "chapter.empty_body")
        .expect("静音过的类别要留下记录");
    assert_eq!(row["enabled"], false, "{row}");
    ok(dir.path(), "question-unmute", &[("template", "chapter.empty_body")]);
    let lifted = ok(dir.path(), "question-weights", &[]);
    let row = lifted["weights"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["template_key"] == "chapter.empty_body")
        .unwrap();
    assert_eq!(row["enabled"], true, "解除静音之后又启用：{row}");

    // 「我自己想起来再问」的回头路：别等了 → 当场回候选池、队列里不再挂着它
    let again = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "回头再说"), ("template", "review.recent_chapter"), ("linked", &anchor)],
    )["card_id"]
        .as_i64()
        .unwrap();
    ok(dir.path(), "card-move", &[("id", &again.to_string()), ("to", "asked"), ("trigger", "push")]);
    ok(
        dir.path(),
        "question-defer",
        &[("id", &again.to_string()), ("preset", "only_when_asked"), ("note", "等我缓过来")],
    );
    assert_eq!(ok(dir.path(), "question-deferrals", &[("work", &work)])["count"], 1);
    ok(dir.path(), "question-undefer", &[("id", &again.to_string())]);
    assert_eq!(ok(dir.path(), "question-deferrals", &[("work", &work)])["count"], 0, "取消之后队列里没有它");
    let pending = ok(dir.path(), "card-list", &[("work", &work), ("state", "pending")]);
    assert!(
        pending["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == again),
        "取消延后之后它回到待问：{pending}"
    );

    // 无值开关要真的传得下去（`--auto-derived` 这类：`optional()` 会把空串当成没给）
    let derived = ok(
        dir.path(),
        "card-new",
        &[
            ("work", &work),
            ("body", "由灵感派生的问题"),
            ("template", "chapter.empty_body"),
            ("derived-from", &ours.to_string()),
            ("auto-derived", ""),
        ],
    )["card_id"]
        .as_i64()
        .unwrap();
    let listed = ok(dir.path(), "card-list", &[("work", &work), ("state", "pending")]);
    let row = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["id"] == derived)
        .expect("派生出来的卡要在列表里");
    assert_eq!(row["auto_derived"], true, "开关没传下去就成了作者手动派生：{row}");
}

/// 作答：答案进答案池、卡走到「已答」终态、输入方式如实记下——**正文一个字节都不动**。
#[test]
fn question_answering_is_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let node = chapter_id.to_string();
    ok(dir.path(), "write", &[("node", &node), ("body", "第一章的正文，作答不该动它一个字。")]);
    let before = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();

    // 没点开就答也算答：先补一条「问出」，新颖度照常消耗
    let card = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "他为什么不肯烧那封信？"), ("template", "chapter.empty_body")],
    )["card_id"]
        .as_i64()
        .unwrap();
    let answered = ok(
        dir.path(),
        "question-answer",
        &[("id", &card.to_string()), ("body", "  他怕烧掉就认不出自己  "), ("source", "mixed")],
    );
    let answer_id = answered["answer_id"].as_i64().unwrap();

    // 答案读得回来：原文（修剪过）、输入方式、溯源
    let answers = ok(dir.path(), "question-answers", &[("id", &card.to_string())]);
    assert_eq!(answers["count"], 1, "{answers}");
    assert_eq!(answers["answers"][0]["id"], answer_id);
    assert_eq!(answers["answers"][0]["body"], "他怕烧掉就认不出自己");
    assert_eq!(answers["answers"][0]["source"], "mixed", "输入方式与文本解耦，照原样记下");
    assert_eq!(answers["answers"][0]["card_id"], card, "答案查得到自己答的是哪张卡");

    // 状态走到终态，两条证据（字段 + 事件）都在
    assert_eq!(ok(dir.path(), "card-list", &[("work", &work), ("state", "answered")])["count"], 1);
    let events = ok(dir.path(), "card-events", &[("id", &card.to_string())]);
    let ops: Vec<&str> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, vec!["create", "ask", "answer"], "{events}");

    // 已答是终态：再答一次被状态机拒，原答案与事件都不多不少
    match run(dir.path(), "question-answer", &[("id", &card.to_string()), ("body", "改一版")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "card.illegal_transition"),
        other => panic!("已答的卡不该还能再答：{other:?}"),
    }
    assert_eq!(ok(dir.path(), "question-answers", &[("id", &card.to_string())])["count"], 1);

    // 认不出的输入方式当场拒绝（留痕那一列不能写歪）
    let fresh = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "再来一张"), ("template", "chapter.empty_body")],
    )["card_id"]
        .as_i64()
        .unwrap();
    match run(dir.path(), "question-answer", &[("id", &fresh.to_string()), ("body", "答案"), ("source", "telepathy")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "input.source_unknown"),
        other => panic!("认不出的输入方式该被拒：{other:?}"),
    }
    assert_eq!(ok(dir.path(), "question-answers", &[("id", &fresh.to_string())])["count"], 0);

    // 只问不写：作答前后正文指纹一模一样
    let after = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();
    assert_eq!(before, after, "作答不碰正文");

    // 落章：**只留痕，不写正文**（稿子还是那一个字节，正文那一笔由界面插进编辑会话）
    ok(dir.path(), "question-land", &[("id", &card.to_string()), ("node", &node)]);
    let answers = ok(dir.path(), "question-answers", &[("id", &card.to_string())]);
    assert_eq!(answers["answers"][0]["status"], "landed", "落过正文的答案看得出来：{answers}");
    let landed = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();
    assert_eq!(landed, before, "落章命令自己不写正文");

    // 落点不合法（另一本书的章 / 不承载正文的节点）当场拒
    match run(dir.path(), "question-land", &[("id", &card.to_string()), ("node", "99999")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "node.gone"),
        other => panic!("不存在的落点该被拒：{other:?}"),
    }

    // 「跟着这一章走」：给 --node 时，与这一章有关的问题排最前（其余照样在列表里）
    let anchored = ok(
        dir.path(),
        "card-new",
        &[
            ("work", &work),
            ("body", "这一章的问题"),
            ("template", "chapter.empty_body"),
            ("linked", &format!("chapter:{chapter_id}")),
        ],
    )["card_id"]
        .as_i64()
        .unwrap();
    let picked = ok(dir.path(), "question-select", &[("work", &work), ("node", &node)]);
    assert_eq!(picked["questions"][0]["card_id"], anchored, "这一章的问题排最前：{picked}");
    assert_eq!(picked["questions"][0]["anchors"][0], format!("chapter:{chapter_id}"));
    assert!(
        picked["questions"].as_array().unwrap().iter().any(|q| q["card_id"] == fresh),
        "别的问题照样在列表里，只是排在后面"
    );
}

/// 一轮落章（先问后排版）：一串答案**一次**落进这一章——顺序按给的来，改过的字回写，
/// 稿子仍是一个字节不动。
#[test]
fn question_round_is_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let node = chapter_id.to_string();
    ok(dir.path(), "write", &[("node", &node), ("body", "正文先摆一句话在这儿。")]);
    let before = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();

    // 一轮里答两条
    let mut cards = Vec::new();
    for body in ["第一段：他回了家。", "第二段：信还在抽屉里。"] {
        let card = ok(
            dir.path(),
            "card-new",
            &[("work", &work), ("body", body), ("template", "chapter.empty_body")],
        )["card_id"]
            .as_i64()
            .unwrap();
        ok(dir.path(), "question-answer", &[("id", &card.to_string()), ("body", body)]);
        cards.push(card);
    }

    // 落：顺序倒过来，第二条还改了字
    let items = format!(
        r#"[{{"card_id":{},"body":"第二段：信还在抽屉里（改了字）。"}},{{"card_id":{},"body":"第一段：他回了家。"}}]"#,
        cards[1], cards[0]
    );
    let landed = ok(
        dir.path(),
        "question-round",
        &[("work", &work), ("node", &node), ("items", &items)],
    );
    assert_eq!(landed["count"], 2, "{landed}");

    // 两条都标成落过；改过的那条以托盘里的字为准
    let second = ok(dir.path(), "question-answers", &[("id", &cards[1].to_string())]);
    assert_eq!(second["answers"][0]["status"], "landed");
    assert_eq!(second["answers"][0]["body"], "第二段：信还在抽屉里（改了字）。", "改过的字回写了答案池");

    // 坏输入整轮拒绝：空数组 / 没答案的卡
    match run(dir.path(), "question-round", &[("work", &work), ("node", &node), ("items", "[]")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "round.empty"),
        other => panic!("空轮该被拒：{other:?}"),
    }
    let fresh = ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "还没答的一条"), ("template", "chapter.empty_body")],
    )["card_id"]
        .as_i64()
        .unwrap();
    let bad = format!(r#"[{{"card_id":{},"body":"没答过就落"}}]"#, fresh);
    match run(dir.path(), "question-round", &[("work", &work), ("node", &node), ("items", &bad)]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "answer.not_found"),
        other => panic!("没答案的卡该被拒：{other:?}"),
    }

    // 只做账：稿子一个字节不动（正文那几段字由界面插进编辑会话）
    let after = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();
    assert_eq!(before, after, "一轮落章命令自己不写正文");
}

/// 落点三档：章纲（合并成一行）、场景卡（新建一张）、正文（核心一个字节都不写）。
#[test]
fn answers_land_in_the_three_places_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let node = chapter_id.to_string();
    ok(dir.path(), "write", &[("node", &node), ("body", "正文先摆一句话在这儿。")]);
    let before = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();

    let mut cards = Vec::new();
    for body in ["第三人称，跟着林望", "雨夜，码头", "他站在门口，没敢敲门。"] {
        let card = ok(
            dir.path(),
            "card-new",
            &[("work", &work), ("body", body), ("template", "chapter.empty_body")],
        )["card_id"]
            .as_i64()
            .unwrap();
        ok(dir.path(), "question-answer", &[("id", &card.to_string()), ("body", body)]);
        cards.push(card);
    }

    // ① 章纲：写进这一章的"一句话"
    let first = ok(
        dir.path(),
        "question-land",
        &[("id", &cards[0].to_string()), ("node", &node), ("target", "outline")],
    );
    assert_eq!(first["outline"], "第三人称，跟着林望", "{first}");

    // ② 场景卡：这一章下面新建一张，名字是作者起的
    let second = ok(
        dir.path(),
        "question-land",
        &[
            ("id", &cards[1].to_string()),
            ("node", &node),
            ("target", "scene"),
            ("title", "码头"),
        ],
    );
    let scene_id = second["scene_ids"][0].as_i64().unwrap();
    let chased = ok(dir.path(), "fingerprint", &[("node", &scene_id.to_string())]);
    assert!(chased["fingerprint"].is_string(), "场景卡是一张真的节点：{chased}");

    // ③ 正文：核心不写（正文由界面插进编辑会话）
    ok(dir.path(), "question-land", &[("id", &cards[2].to_string()), ("node", &node)]);

    // 三条都标成落过；稿子（这一章正文）一个字节没动
    for card in &cards {
        let answers = ok(dir.path(), "question-answers", &[("id", &card.to_string())]);
        assert_eq!(answers["answers"][0]["status"], "landed", "{answers}");
    }
    let after = ok(dir.path(), "fingerprint", &[("node", &node)])["fingerprint"].clone();
    assert_eq!(before, after, "落章命令自己不写这一章的正文");

    // 认不出的落点当场拒
    match run(dir.path(), "question-land", &[("id", &cards[0].to_string()), ("node", &node), ("target", "telepathy")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "answer.target_unknown"),
        other => panic!("认不出的落点该被拒：{other:?}"),
    }
}

/// 主动问一句（推）：**过了门槛才开口，开口就算问过**——配额只限推，面板不受它管。
#[test]
fn question_push_is_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let node = chapter_id.to_string();
    ok(
        dir.path(),
        "card-new",
        &[("work", &work), ("body", "推这一条"), ("template", "chapter.empty_body")],
    );

    // 一天最多两次、无冷却：前两次问得出去，第三次被配额挡住
    let mut seen = Vec::new();
    for _ in 0..2 {
        ok(
            dir.path(),
            "card-new",
            &[("work", &work), ("body", "再来一条"), ("template", "review.recent_chapter")],
        );
        let pushed = ok(
            dir.path(),
            "question-push",
            &[("work", &work), ("node", &node), ("per-day", "2"), ("cooldown", "0"), ("today", "20260916")],
        );
        assert_eq!(pushed["code"], "push.asked", "{pushed}");
        seen.push(pushed["question"]["card_id"].as_i64().unwrap());
    }
    assert_ne!(seen[0], seen[1], "推走的两条不是同一张");

    let blocked = ok(
        dir.path(),
        "question-push",
        &[("work", &work), ("node", &node), ("per-day", "2"), ("cooldown", "0"), ("today", "20260916")],
    );
    assert_eq!(blocked["code"], "push.quota_used", "{blocked}");

    // 第二天：配额重新开始
    let tomorrow = ok(
        dir.path(),
        "question-push",
        &[("work", &work), ("node", &node), ("per-day", "2"), ("cooldown", "0"), ("today", "20260917")],
    );
    assert_eq!(tomorrow["code"], "push.asked", "跨天重置：{tomorrow}");

    // 0 次 = 不打扰：一次都不问
    let quiet = ok(
        dir.path(),
        "question-push",
        &[("work", &work), ("node", &node), ("per-day", "0"), ("cooldown", "0"), ("today", "20260917")],
    );
    assert_eq!(quiet["code"], "push.quota_used", "{quiet}");

    // 推过的算「已问」：面板那份池子里不再有它（而配额只管推）
    let asked = ok(dir.path(), "card-list", &[("work", &work), ("state", "asked")]);
    assert!(asked["count"].as_i64().unwrap() >= 1, "推走的卡要走到「已问」：{asked}");
}

/// 创作流碎片：**记 / 看 / 删 / 捞回**——界面上"删了能捞回""不该建的建不成"这类事
/// 要能从命令行反复驱动（界面做不成自动化）。
#[test]
fn fragment_pool_is_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();

    // 记两条：正文两边的空白会被修剪，锚点可以一次给多个（逗号分隔）
    let idea = ok(
        dir.path(),
        "fragment-add",
        &[
            ("work", &work),
            ("kind", "idea"),
            ("body", "  一个念头  "),
            ("anchor", &format!("chapter:{chapter_id},chapter:999")),
        ],
    );
    let idea_id = idea["fragment"]["id"].as_i64().unwrap();
    assert_eq!(idea["fragment"]["body"], "一个念头", "存的是修剪过的那一句");
    assert_eq!(idea["fragment"]["kind"], "idea");
    assert_eq!(idea["fragment"]["source"], "typed");
    assert_eq!(idea["fragment"]["anchors"].as_array().unwrap().len(), 2);

    ok(dir.path(), "fragment-add", &[("work", &work), ("kind", "event"), ("body", "他走进来")]);

    // 面板：新的在前，各档的数字与列表同一口径（没记过的种类也如实回 0）
    let board = ok(dir.path(), "fragment-board", &[("work", &work)]);
    assert_eq!(board["fragments"].as_array().unwrap().len(), 2);
    assert_eq!(board["fragments"][0]["kind"], "event", "最近记的在最前：{board}");
    let counts = board["counts"].as_array().unwrap();
    assert!(counts.iter().any(|item| item["kind"] == "idea" && item["count"] == 1));
    assert!(counts.iter().any(|item| item["kind"] == "dictation" && item["count"] == 0));

    // 问题与答案归叩问那条线：从这儿建**当场被拒**
    let refused = run(dir.path(), "fragment-add", &[("work", &work), ("kind", "question"), ("body", "不该从这儿建")]);
    match refused {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "fragment.kind_not_jotted"),
        other => panic!("问题卡不该从创作流建：{other:?}"),
    }

    // 认不出的种类、空正文也都拒
    assert!(run(dir.path(), "fragment-add", &[("work", &work), ("kind", "memo"), ("body", "一句")]).is_err());
    assert!(run(dir.path(), "fragment-add", &[("work", &work), ("kind", "idea"), ("body", "   ")]).is_err());

    // 删是软删：列表与计数都不算它；捞回就是把时间戳抹掉
    ok(dir.path(), "fragment-delete", &[("id", &idea_id.to_string())]);
    let after_delete = ok(dir.path(), "fragment-board", &[("work", &work)]);
    assert_eq!(after_delete["fragments"].as_array().unwrap().len(), 1);
    assert!(after_delete["counts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["kind"] == "idea" && item["count"] == 0));

    ok(dir.path(), "fragment-restore", &[("id", &idea_id.to_string())]);
    let after_restore = ok(dir.path(), "fragment-board", &[("work", &work)]);
    assert_eq!(after_restore["fragments"].as_array().unwrap().len(), 2, "捞回之后还在：{after_restore}");

    // 删两次 / 删不存在的：如实说「不存在」，不静默当成删成功
    ok(dir.path(), "fragment-delete", &[("id", &idea_id.to_string())]);
    match run(dir.path(), "fragment-delete", &[("id", &idea_id.to_string())]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "fragment.not_found"),
        other => panic!("删两次该说「不存在」：{other:?}"),
    }
}

/// 大纲这一条线：**记设定 → 体检 → 忽略 → 数据一变清单跟着变**。
///
/// 界面上要点得动的事，这里都要能反复跑：重名会不会报、忽略记没记住、
/// 软删的那张还报不报、场景卡填全了缺项还在不在。
#[test]
fn outline_cards_and_the_checkup_are_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let scene = ok(
        dir.path(),
        "new-node",
        &[("work", &work), ("parent", &chapter_id.to_string()), ("kind", "scene"), ("title", "开场")],
    );
    let scene_id = scene["node_id"].as_i64().unwrap().to_string();
    // 四格填**一格**：这样它才算"填了一半"（一个字都没填的不在体检里念——
    // 一本没规划过的书会在清单里刷出一百条，把真正该看的埋掉）
    ok(dir.path(), "scene-field", &[("node", &scene_id), ("pov", "陆文")]);

    // 两张卡撞一个别称；其中一张自己跟自己矛盾（发色两种说法）
    let first = ok(
        dir.path(),
        "entity-new",
        &[
            ("work", &work),
            ("kind", "person"),
            ("name", "陆文"),
            ("alias", "阿文,陆大人"),
            ("attr", "发色=黑,发色=白"),
        ],
    );
    let first_id = first["card"]["id"].as_i64().unwrap();
    assert_eq!(first["card"]["aliases"].as_array().unwrap().len(), 2);
    ok(
        dir.path(),
        "entity-new",
        &[("work", &work), ("kind", "person"), ("name", "林昭"), ("alias", "阿文")],
    );

    // 体检（只读）：三类都在
    let board = ok(dir.path(), "outline-scan", &[("work", &work)]);
    assert_eq!(board["count"].as_i64().unwrap(), 3, "{board}");
    let issues = board["issues"].as_array().unwrap();
    let rules: Vec<&str> = issues.iter().map(|item| item["rule"].as_str().unwrap()).collect();
    assert!(rules.contains(&"entity.name_clash"));
    assert!(rules.contains(&"entity.attribute_conflict"));
    assert!(rules.contains(&"scene.missing_fields"));
    let fingerprint = issues[0]["fingerprint"].as_str().unwrap().to_string();

    // 忽略要**记住**，而且是幂等的
    let dismissed = ok(dir.path(), "outline-dismiss", &[("work", &work), ("fingerprint", &fingerprint)]);
    assert_eq!(dismissed["dismissed"].as_array().unwrap().len(), 1);
    let again = ok(dir.path(), "outline-dismiss", &[("work", &work), ("fingerprint", &fingerprint)]);
    assert_eq!(again["dismissed"].as_array().unwrap().len(), 1, "连点两下不算错");
    let back = ok(dir.path(), "outline-undismiss", &[("work", &work), ("fingerprint", &fingerprint)]);
    assert!(back["dismissed"].as_array().unwrap().is_empty(), "回头路要通");

    // 软删那张撞车的卡：重名那条跟着消失
    ok(dir.path(), "entity-delete", &[("id", &first_id.to_string())]);
    let after = ok(dir.path(), "outline-scan", &[("work", &work)]);
    let rules: Vec<&str> = after["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["rule"].as_str().unwrap())
        .collect();
    assert!(!rules.contains(&"entity.name_clash"), "删掉的那张不该再报：{after}");
    assert!(!rules.contains(&"entity.attribute_conflict"), "同一张卡上的属性冲突也一样：{after}");

    // 填全四格：缺项那条消失
    ok(
        dir.path(),
        "scene-field",
        &[
            ("node", &scene_id),
            ("pov", "陆文"),
            ("goal", "拿到账本"),
            ("conflict", "他不肯给"),
            ("outcome", "抢到了"),
        ],
    );
    let filled = ok(dir.path(), "outline-scan", &[("work", &work)]);
    assert_eq!(filled["count"].as_i64().unwrap(), 0, "{filled}");

    // **章也有四格**（中文网文的习惯就是一章一行），卷没有——那种如实拒
    ok(dir.path(), "scene-field", &[("node", &chapter_id.to_string()), ("pov", "陆文")]);
    match run(dir.path(), "scene-field", &[("node", "1"), ("pov", "谁")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "node.no_fields"),
        other => panic!("卷不该有四格：{other:?}"),
    }
}

/// 出场人物与整片粘贴：**命令行也能走完**（界面上那一格点得动的事，这里要能反复跑）。
///
/// 盯三件事：名单是**整份覆盖**（给了谁就是谁、不写就是清空）、
/// 表里那一行真带上了人、整片粘贴要么全落要么一格都不落。
#[test]
fn cast_and_a_pasted_block_are_drivable_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let (work_id, chapter_id) = seed(dir.path(), "novel");
    let work = work_id.to_string();
    let chapter = chapter_id.to_string();
    let lu = ok(dir.path(), "entity-new", &[("work", &work), ("kind", "person"), ("name", "陆文")]);
    let zhang = ok(dir.path(), "entity-new", &[("work", &work), ("kind", "person"), ("name", "老张")]);
    let lu_id = lu["card"]["id"].as_i64().unwrap();
    let zhang_id = zhang["card"]["id"].as_i64().unwrap();

    // 挂两个人：读回来按名字排
    let both = format!("{lu_id},{zhang_id}");
    let set = ok(dir.path(), "cast-set", &[("node", &chapter), ("entity", &both)]);
    assert_eq!(set["cast"].as_array().unwrap().len(), 2, "{set}");
    assert_eq!(set["cast"][0]["name"].as_str().unwrap(), "老张");

    let list = ok(dir.path(), "cast-list", &[("work", &work)]);
    assert_eq!(list["count"].as_i64().unwrap(), 1, "只有那一章挂过人：{list}");
    assert_eq!(list["nodes"][0]["node_id"].as_i64().unwrap(), chapter_id);

    // 大纲表那一屏也带着人（界面读的就是它）
    let rows = ok(dir.path(), "outline-rows", &[("work", &work)]);
    let row = rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["node_id"].as_i64() == Some(chapter_id))
        .unwrap();
    assert_eq!(row["cast"].as_array().unwrap().len(), 2, "{rows}");

    // 整份覆盖：只留陆文；再不写 --entity 就是清空
    let only_lu = ok(dir.path(), "cast-set", &[("node", &chapter), ("entity", &lu_id.to_string())]);
    assert_eq!(only_lu["cast"].as_array().unwrap().len(), 1);
    let cleared = ok(dir.path(), "cast-set", &[("node", &chapter)]);
    assert!(cleared["cast"].as_array().unwrap().is_empty(), "不写 --entity = 谁也不出场");

    // 整片粘贴：两格落在同一章上（一句话 + 视角）
    let cells = format!(
        "[{{\"node_id\":{chapter_id},\"column\":\"summary\",\"value\":\"他第一次进城\"}},\
          {{\"node_id\":{chapter_id},\"column\":\"pov\",\"value\":\"陆文\"}}]"
    );
    let path = dir.path().join("cells.json");
    std::fs::write(&path, cells).unwrap();
    let pasted = ok(
        dir.path(),
        "paste-cells",
        &[("work", &work), ("cells-file", path.to_str().unwrap())],
    );
    assert_eq!(pasted["cells"].as_i64().unwrap(), 2);

    let rows = ok(dir.path(), "outline-rows", &[("work", &work)]);
    let row = rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["node_id"].as_i64() == Some(chapter_id))
        .unwrap();
    assert_eq!(row["summary"].as_str().unwrap(), "他第一次进城");
    assert_eq!(row["fields"]["pov"].as_str().unwrap(), "陆文");

    // 一片里有一格落不了（落到卷上）：整片都不落——那一章的一句话保持原样
    let volume_id = rows["rows"][0]["node_id"].as_i64().unwrap();
    let bad = format!(
        "[{{\"node_id\":{chapter_id},\"column\":\"summary\",\"value\":\"不该写进去\"}},\
          {{\"node_id\":{volume_id},\"column\":\"goal\",\"value\":\"卷没有这一格\"}}]"
    );
    std::fs::write(&path, bad).unwrap();
    match run(
        dir.path(),
        "paste-cells",
        &[("work", &work), ("cells-file", path.to_str().unwrap())],
    ) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "node.no_fields"),
        other => panic!("一片里有一格落不了就该整片拒：{other:?}"),
    }
    let rows = ok(dir.path(), "outline-rows", &[("work", &work)]);
    let row = rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["node_id"].as_i64() == Some(chapter_id))
        .unwrap();
    assert_eq!(row["summary"].as_str().unwrap(), "他第一次进城", "拒了就不许留下半片");
}

/// 大纲体检第二刀：**伏笔的埋/收 + 事件的故事时间**。
/// 这一段盯的是"只有结构化记下来才判得动"的两条规则：
/// 埋久了没收的伏笔、后一章却故事时间更早的事件；以及"不写了"是正经结局、
/// 非法边与脏锚点都说人话。
#[test]
fn foreshadows_and_story_time_drive_the_checkup() {
    let dir = tempfile::tempdir().unwrap();
    let work = ok(dir.path(), "new-work", &[("kind", "novel"), ("title", "长夜")]);
    let work_id = work["work_id"].as_i64().unwrap();
    let work = work_id.to_string();
    // 卷是根节点的第一卷；往里塞 25 章（体检的阈值是"隔了 20 章还没收"）
    let volume = ok(dir.path(), "nodes", &[("work", &work)]).to_string();
    assert!(!volume.is_empty());
    let mut chapters: Vec<i64> = Vec::new();
    for index in 1..=25 {
        let chapter = ok(
            dir.path(),
            "new-node",
            &[
                ("work", &work),
                ("parent", "1"),
                ("kind", "chapter"),
                ("title", &format!("第{index}章")),
            ],
        );
        chapters.push(chapter["node_id"].as_i64().unwrap());
    }

    // 一条伏笔埋在第 1 章：隔了 24 章还没收 → 报
    let foreshadow = ok(
        dir.path(),
        "foreshadow-new",
        &[("work", &work), ("body", "老张的怀表"), ("node", &chapters[0].to_string())],
    );
    let foreshadow_id = foreshadow["item"]["id"].as_i64().unwrap();
    assert_eq!(foreshadow["item"]["state"], "planted");

    // 两条带故事时间的事件：第 1 章是第 99 天，第 5 章反而是第 12 天 → 报倒置
    let first = ok(
        dir.path(),
        "fragment-add",
        &[("work", &work), ("kind", "event"), ("body", "开场"), ("anchor", &format!("chapter:{}", chapters[0]))],
    );
    let second = ok(
        dir.path(),
        "fragment-add",
        &[("work", &work), ("kind", "event"), ("body", "后面那件"), ("anchor", &format!("chapter:{}", chapters[4]))],
    );
    ok(
        dir.path(),
        "fragment-edit",
        &[
            ("id", &first["fragment"]["id"].as_i64().unwrap().to_string()),
            ("body", "开场"),
            ("story-time", "承平三年·春"),
            ("story-order", "99"),
        ],
    );
    ok(
        dir.path(),
        "fragment-edit",
        &[
            ("id", &second["fragment"]["id"].as_i64().unwrap().to_string()),
            ("body", "后面那件"),
            ("story-order", "12"),
        ],
    );

    let board = ok(dir.path(), "outline-scan", &[("work", &work)]);
    let rules: Vec<&str> = board["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["rule"].as_str().unwrap())
        .collect();
    assert!(rules.contains(&"foreshadow.uncollected"), "{board}");
    assert!(rules.contains(&"timeline.out_of_order"), "{board}");

    // 收了那条伏笔（收在第 20 章）：伏笔那条消失
    let collected = ok(
        dir.path(),
        "foreshadow-move",
        &[("id", &foreshadow_id.to_string()), ("to", "collected"), ("collected-node", &chapters[19].to_string())],
    );
    assert_eq!(collected["item"]["state"], "collected");
    assert_eq!(collected["item"]["collected_node"].as_i64().unwrap(), chapters[19]);
    let after = ok(dir.path(), "outline-scan", &[("work", &work)]);
    assert!(
        !after["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["rule"] == "foreshadow.uncollected"),
        "收了的伏笔不该再报：{after}"
    );

    // 倒叙标记：同一条事件标成回忆之后，倒置那条也不报了（作者自己的写法）
    ok(
        dir.path(),
        "fragment-edit",
        &[
            ("id", &second["fragment"]["id"].as_i64().unwrap().to_string()),
            ("body", "后面那件"),
            ("story-order", "12"),
            ("flashback", ""),
        ],
    );
    let flashed = ok(dir.path(), "outline-scan", &[("work", &work)]);
    assert!(
        !flashed["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["rule"] == "timeline.out_of_order"),
        "标成倒叙就不该再报：{flashed}"
    );

    // 非法边 / 脏锚点：都是**说人话的码**，不是数据库约束失败
    match run(dir.path(), "foreshadow-move", &[("id", &foreshadow_id.to_string()), ("to", "dropped")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "foreshadow.illegal_transition"),
        other => panic!("收了之后不能直接跳到「不写了」：{other:?}"),
    }
    match run(dir.path(), "foreshadow-new", &[("work", &work), ("body", "挂到不存在的一章"), ("node", "99999")]) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "foreshadow.anchor_invalid"),
        other => panic!("脏锚点要如实拒：{other:?}"),
    }
    // 灵感卡带故事时间：当场拒（不是事件）
    let idea = ok(dir.path(), "fragment-add", &[("work", &work), ("kind", "idea"), ("body", "一个念头")]);
    match run(
        dir.path(),
        "fragment-edit",
        &[("id", &idea["fragment"]["id"].as_i64().unwrap().to_string()), ("body", "一个念头"), ("story-order", "3")],
    ) {
        Err(CliError::Core(error)) => assert_eq!(error.code(), "fragment.story_time_not_event"),
        other => panic!("灵感卡不该有故事时间：{other:?}"),
    }
}
