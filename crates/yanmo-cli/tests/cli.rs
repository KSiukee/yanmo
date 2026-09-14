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
