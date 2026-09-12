//! 开发档命令：**只存在于开发（debug）构建**——发布构建里整个模块都不编译。
//!
//! 它们存在的理由不是"给作者用"，而是"让事故能被复现"：
//! 起一次会话、写一段字、把进程保持开着、从外面中断它、再重启看结果——
//! 这几步走界面做不成自动化（界面要人点），所以要有一条命令行能一次走完。
//!
//! 发布版把它们剃掉是有意的：**命令面越小，需要被信任的代码就越少**。

use serde_json::{json, Value};
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{SessionReport, Store};
use yanmo_core::text;

use crate::args::{Args, Usage};
use crate::CliError;

/// 开发档命令允许哪些选项（发布构建里没人会调到这里）。
pub fn options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "begin" | "report" | "abandon" => Some(&[]),
        "note-open" | "fingerprint" | "end" => Some(&["node"]),
        "write" => Some(&["node", "body", "body-file"]),
        "new-work" => Some(&["kind", "title"]),
        "new-node" => Some(&["work", "parent", "kind", "title"]),
        "hold" => Some(&["node", "seconds"]),
        _ => None,
    }
}

fn session_json(report: SessionReport) -> Value {
    json!({
        "unclean": report.unclean,
        "last_node_id": report.last_node_id,
        "last_seen_at": report.last_seen_at,
    })
}

/// 执行开发档命令；不是这一档就返回 `None`。
pub fn execute(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    let value = match args.command.as_str() {
        "begin" => {
            let report = store.begin_session()?;
            json!({ "ok": true, "command": "begin", "session": session_json(report) })
        }
        "report" => {
            let report = store.peek_session()?;
            json!({ "ok": true, "command": "report", "session": session_json(report) })
        }
        "note-open" => {
            let node = args.required_i64("node")?;
            store.note_open_node(node)?;
            json!({ "ok": true, "command": "note-open", "node_id": node })
        }
        "write" => {
            let node = args.required_i64("node")?;
            let body = body_of(args)?;
            let stats = store.write_body(node, &body)?;
            json!({
                "ok": true,
                "command": "write",
                "node_id": node,
                "char_count": stats.char_count,
                "word_count": stats.word_count,
                "fingerprint": text::content_hash(&body),
            })
        }
        "fingerprint" => {
            let node = args.required_i64("node")?;
            let fingerprint = store.body_fingerprint(node)?;
            json!({ "ok": true, "command": "fingerprint", "node_id": node, "fingerprint": fingerprint })
        }
        "end" => {
            let node = args.required_i64("node")?;
            let written = store.end_session(node)?;
            json!({ "ok": true, "command": "end", "snapshot_written": written })
        }
        "abandon" => {
            store.abandon_session()?;
            json!({ "ok": true, "command": "abandon" })
        }
        "new-work" => {
            let kind = WorkKind::parse(args.required("kind")?)?;
            let work = store.create_work(kind, args.required("title")?)?;
            json!({ "ok": true, "command": "new-work", "work_id": work.id })
        }
        "new-node" => {
            let work = args.required_i64("work")?;
            let parent = match args.optional("parent") {
                Some(text) => Some(
                    text.parse::<i64>().map_err(|_| Usage::from("--parent 需要是一个整数"))?,
                ),
                None => None,
            };
            let kind = NodeKind::parse(args.required("kind")?)?;
            let id = store.create_node(work, parent, kind, args.optional("title").unwrap_or(""))?;
            json!({ "ok": true, "command": "new-node", "node_id": id })
        }
        "hold" => {
            let node = args.required_i64("node")?;
            let seconds = match args.optional("seconds") {
                Some(text) => {
                    text.parse::<u64>().map_err(|_| Usage::from("--seconds 需要是一个整数"))?
                }
                None => 30,
            };
            store.note_open_node(node)?;
            // 把这次会话**保持开着**：调用方（脚本 / 人）要的就是"运行中"这个状态，
            // 然后从外面把它中断掉。到时间自然退出也算一次正常结束。
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            json!({ "ok": true, "command": "hold", "node_id": node, "seconds": seconds })
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// 正文从哪来：`--body` 直接给，或 `--body-file` 从文件读（长文本用后者）。
fn body_of(args: &Args) -> Result<String, CliError> {
    if let Some(path) = args.optional("body-file") {
        return Ok(std::fs::read_to_string(path)?);
    }
    Ok(args.required("body")?.to_string())
}
