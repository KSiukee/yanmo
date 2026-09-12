//! 各命令的执行：**只做「参数转换 + 调核心 + 组装 JSON」**。
//!
//! 业务一律在 `yanmo-core`：这里没有第二条数据逻辑，也不拼人类句子——
//! 成功给数据、失败给「码 + 参数」，怎么讲由调用方（脚本 / 人）自己决定。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use yanmo_core::db;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{ExportFormat, SessionReport, Store};
use yanmo_core::text;
use yanmo_core::version;

use crate::args::{Args, Usage};
use crate::CliError;

/// 数据目录里的库文件名（与桌面壳同一份约定，见 `app/src/storage.rs`）。
const DB_FILE: &str = "yanmo.db";

/// 每个命令允许哪些选项——**写错一个字母会当场报错**，不会被静默忽略。
///
/// 调试用的那几个选项只在带测试特性编译时才是"已知选项"；
/// 发布构建里传它们会落到"不认识选项"上（解析分支根本不存在）。
fn allowed_options(command: &str) -> Result<Vec<&'static str>, Usage> {
    let known: &[&str] = match command {
        "begin" | "report" | "verify" | "abandon" => &[],
        "note-open" | "read" | "fingerprint" | "end" => &["node"],
        "write" => &["node", "body", "body-file"],
        "new-work" => &["kind", "title"],
        "new-node" => &["work", "parent", "kind", "title"],
        "nodes" => &["work"],
        "hold" => &["node", "seconds"],
        "export" => &["work", "format", "out"],
        other => return Err(Usage(format!("不认识的命令：{other}"))),
    };
    #[allow(unused_mut)]
    let mut known = known.to_vec();
    #[cfg(feature = "testing")]
    known.push("max-pages");
    Ok(known)
}

/// 打开数据目录里的库（目录不存在就建），完成环境校验与结构迁移。
fn open_store(dir: &Path) -> Result<Store, CliError> {
    std::fs::create_dir_all(dir)?;
    Ok(Store::open(dir.join(DB_FILE))?)
}

/// 调试用的注入参数：给库设一个**页数上限**，让后续写入真的走到"写不下"这条路。
///
/// 只在带测试特性编译时存在；发布构建里这个函数体是空的（参数本身也没登记）。
#[cfg(feature = "testing")]
fn apply_debug_limits(store: &Store, args: &Args) -> Result<(), CliError> {
    if let Some(text) = args.options.get("max-pages").filter(|value| !value.is_empty()) {
        let pages: i64 = text
            .parse()
            .map_err(|_| Usage::from("--max-pages 需要是一个整数"))?;
        store
            .conn()
            .pragma_update(None, "max_page_count", pages)
            .map_err(yanmo_core::Error::Db)?;
    }
    Ok(())
}

#[cfg(not(feature = "testing"))]
fn apply_debug_limits(_store: &Store, _args: &Args) -> Result<(), CliError> {
    Ok(())
}

fn session_json(report: SessionReport) -> Value {
    json!({
        "unclean": report.unclean,
        "last_node_id": report.last_node_id,
        "last_seen_at": report.last_seen_at,
    })
}

/// 执行一条命令，产出要打印的 JSON。
pub fn execute(args: &Args) -> Result<Value, CliError> {
    for name in args.options.keys() {
        if !allowed_options(&args.command)?.contains(&name.as_str()) {
            return Err(Usage(format!("命令 {} 不认识选项 --{name}", args.command)).into());
        }
    }

    // 先看命令要不要库：`verify` 也要，而且它自己会报告"库坏在哪"
    let mut store = open_store(&args.data)?;
    apply_debug_limits(&store, args)?;

    match args.command.as_str() {
        "begin" => {
            let report = store.begin_session()?;
            Ok(json!({ "ok": true, "command": "begin", "session": session_json(report) }))
        }
        "report" => {
            let report = store.peek_session()?;
            Ok(json!({ "ok": true, "command": "report", "session": session_json(report) }))
        }
        "note-open" => {
            let node = args.required_i64("node")?;
            store.note_open_node(node)?;
            Ok(json!({ "ok": true, "command": "note-open", "node_id": node }))
        }
        "write" => {
            let node = args.required_i64("node")?;
            let body = body_of(args)?;
            let stats = store.write_body(node, &body)?;
            Ok(json!({
                "ok": true,
                "command": "write",
                "node_id": node,
                "char_count": stats.char_count,
                "word_count": stats.word_count,
                "fingerprint": text::content_hash(&body),
            }))
        }
        "read" => {
            let node = args.required_i64("node")?;
            let body = store.read_body(node)?;
            Ok(json!({ "ok": true, "command": "read", "node_id": node, "body": body }))
        }
        "fingerprint" => {
            let node = args.required_i64("node")?;
            let fingerprint = store.body_fingerprint(node)?;
            Ok(json!({ "ok": true, "command": "fingerprint", "node_id": node, "fingerprint": fingerprint }))
        }
        "end" => {
            let node = args.required_i64("node")?;
            let written = store.end_session(node)?;
            Ok(json!({ "ok": true, "command": "end", "snapshot_written": written }))
        }
        "abandon" => {
            store.abandon_session()?;
            Ok(json!({ "ok": true, "command": "abandon" }))
        }
        "new-work" => {
            let kind = WorkKind::parse(args.required("kind")?)?;
            let work = store.create_work(kind, args.required("title")?)?;
            Ok(json!({ "ok": true, "command": "new-work", "work_id": work.id }))
        }
        "new-node" => {
            let work = args.required_i64("work")?;
            let parent = match args.optional("parent") {
                Some(text) => Some(
                    text.parse::<i64>()
                        .map_err(|_| Usage::from("--parent 需要是一个整数"))?,
                ),
                None => None,
            };
            let kind = NodeKind::parse(args.required("kind")?)?;
            let id = store.create_node(work, parent, kind, args.optional("title").unwrap_or(""))?;
            Ok(json!({ "ok": true, "command": "new-node", "node_id": id }))
        }
        "nodes" => {
            let work = args.required_i64("work")?;
            let nodes: Vec<Value> = store
                .list_nodes(work)?
                .into_iter()
                .map(|node| {
                    json!({
                        "id": node.id,
                        "parent_id": node.parent_id,
                        "kind": node.kind.as_str(),
                        "title": node.title,
                        "word_count": node.word_count,
                        "has_body": node.has_body,
                        "has_children": node.has_children,
                    })
                })
                .collect();
            Ok(json!({ "ok": true, "command": "nodes", "nodes": nodes }))
        }
        "hold" => {
            let node = args.required_i64("node")?;
            let seconds = match args.optional("seconds") {
                Some(text) => text
                    .parse::<u64>()
                    .map_err(|_| Usage::from("--seconds 需要是一个整数"))?,
                None => 30,
            };
            store.note_open_node(node)?;
            // 把这次会话**保持开着**：调用方（脚本 / 人）要的就是"运行中"这个状态，
            // 然后从外面把它中断掉。到时间自然退出也算一次正常结束。
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            Ok(json!({ "ok": true, "command": "hold", "node_id": node, "seconds": seconds }))
        }
        "export" => {
            let work = args.required_i64("work")?;
            let format = ExportFormat::parse(args.required("format")?)?;
            let out = PathBuf::from(args.required("out")?);
            let written = write_rendered(&store, work, format, &out)?;
            Ok(json!({ "ok": true, "command": "export", "files": written, "dir": out.display().to_string() }))
        }
        "verify" => {
            // 打开这一步就已经跑过环境校验与迁移：库坏了 / 版本过新都会在这里明确报错
            let integrity = db::quick_check(store.conn())?;
            let schema = db::migrations::user_version(store.conn())?;
            Ok(json!({
                "ok": true,
                "command": "verify",
                "integrity": integrity,
                "schema_version": schema,
                "engine_version": version::engine_version(),
            }))
        }
        other => Err(Usage(format!("不认识的命令：{other}")).into()),
    }
}

/// 正文从哪来：`--body` 直接给，或 `--body-file` 从文件读（长文本用后者）。
fn body_of(args: &Args) -> Result<String, CliError> {
    if let Some(path) = args.optional("body-file") {
        return Ok(std::fs::read_to_string(path)?);
    }
    Ok(args.required("body")?.to_string())
}

/// 把渲染好的文件写到导出目录：**内容一样就不重写**（与桌面壳同一条纪律）。
fn write_rendered(
    store: &Store,
    work_id: i64,
    format: ExportFormat,
    out: &Path,
) -> Result<usize, CliError> {
    let files = store.render_work(work_id, format)?;
    let mut written = 0;
    for file in &files {
        let path = out.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let unchanged = std::fs::read_to_string(&path).map(|old| old == file.content).unwrap_or(false);
        if unchanged {
            continue;
        }
        yanmo_core::atomic::write_atomic(&path, file.content.as_bytes())?;
        written += 1;
    }
    Ok(written)
}
