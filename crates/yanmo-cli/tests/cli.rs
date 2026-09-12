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
