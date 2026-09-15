//! 开发档命令：**只存在于开发（debug）构建**——发布构建里整个模块都不编译。
//!
//! 它们存在的理由不是"给作者用"，而是"让事故能被复现"：
//! 起一次会话、写一段字、把进程保持开着、从外面中断它、再重启看结果——
//! 这几步走界面做不成自动化（界面要人点），所以要有一条命令行能一次走完。
//!
//! 发布版把它们剃掉是有意的：**命令面越小，需要被信任的代码就越少**。

use serde_json::{json, Value};
use yanmo_core::model::{NewQuestionCard, NodeKind, QuestionState, WorkKind};
use yanmo_core::store::{BackupRequest, BackupTarget, SessionReport, Store};
use yanmo_core::text;

use crate::args::{Args, Usage};
use crate::CliError;

/// 开发档命令允许哪些选项（发布构建里没人会调到这里）。
pub fn options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "begin" | "report" | "abandon" => Some(&[]),
        "note-open" | "fingerprint" | "end" => Some(&["node"]),
        "write" => Some(&["node", "body", "body-file", "tz"]),
        "backup" => Some(&["to", "keep", "tz", "device"]),
        "new-work" => Some(&["kind", "title"]),
        "new-node" => Some(&["work", "parent", "kind", "title"]),
        "hold" => Some(&["node", "seconds"]),
        // 叩问·问题卡：建卡 / 迁移 / 列卡 / 看迁移史。给外部演练台从命令行驱动 6 态状态机，
        // 每一条迁移的证据（fragments.status + op-log）都能被外面独立核对。
        "card-new" => Some(&["work", "body", "body-file", "source", "template", "importance", "derived-from"]),
        "card-move" => Some(&["id", "to", "trigger"]),
        "card-list" => Some(&["work", "state"]),
        "card-events" => Some(&["id"]),
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
            // 给了 `--tz <分钟>` 就走**编辑器那条路**（顺带记进「每日码字」账本）；
            // 不给就只写正文——备份恢复、脚本灌数据这类"不是作者今天敲的字"走这条路。
            // 时区偏移只有调用方知道（核心不猜作者在哪），东八区是 480。
            let stats = match args.optional("tz") {
                None => store.write_body(node, &body)?,
                Some(text) => {
                    let tz: i32 = text
                        .parse()
                        .map_err(|_| Usage::from("--tz 需要是一个整数（分钟，东八区 480）"))?;
                    store.write_body_counted(node, &body, tz)?
                }
            };
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
        "backup" => {
            // 演练用：把「多处备份」这条链从**外部**驱动起来（备份平时只挂在界面命令上，
            // 而七层防线里的"备份目标不可写会怎样"必须有人能从外面验）。
            // 与别的写库命令一样：只在开发构建里存在，发行版连解析分支都没有。
            let to = std::path::PathBuf::from(args.required("to")?);
            let keep = match args.optional("keep") {
                None => 7usize,
                Some(text) => text
                    .parse()
                    .map_err(|_| Usage::from("--keep 需要是一个整数（每个目标留几份）"))?,
            };
            let tz: i32 = match args.optional("tz") {
                None => 0,
                Some(text) => text
                    .parse()
                    .map_err(|_| Usage::from("--tz 需要是一个整数（分钟，东八区 480）"))?,
            };
            let request = BackupRequest {
                data_dir: args.data.clone(),
                targets: vec![BackupTarget {
                    path: to.display().to_string(),
                    // 卷标识由壳从 Windows 卷信息里取；命令行给不出来，留空（只影响"异盘提醒"）
                    volume_id: String::new(),
                    volume_label: String::new(),
                    removable: false,
                }],
                keep,
                tz_offset_minutes: tz,
                device: args.optional("device").unwrap_or("cli").to_string(),
            };
            let report = store.backup_now(&request)?;
            json!({
                "ok": true,
                "command": "backup",
                "stamp": report.stamp,
                "succeeded": report.succeeded(),
                "skipped": report.skipped(),
                "failed": report.failed(),
                "outcomes": report.outcomes,
            })
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
        // ── 叩问·问题卡：把状态机从外面驱动起来 ──────────────────────────
        //
        // 为什么这几条要在命令行上：六个态的可达性要能从**外部**驱动并核对，
        // 而叩问的处置与作答界面还没做。与别的写库命令一样，
        // 它们只存在于开发构建里——发布版连解析分支都没有。
        "card-new" => {
            let work = args.required_i64("work")?;
            let body = body_of(args)?;
            let importance = match args.optional("importance") {
                None => 0.5,
                Some(text) => text
                    .parse::<f64>()
                    .map_err(|_| Usage::from("--importance 需要是一个 0~1 的小数"))?,
            };
            let derived_from = match args.optional("derived-from") {
                None => None,
                Some(text) => Some(
                    text.parse::<i64>()
                        .map_err(|_| Usage::from("--derived-from 需要是一个整数（卡 id）"))?,
                ),
            };
            let card_id = store.create_question_card(&NewQuestionCard {
                work_id: work,
                body,
                source: args.optional("source").unwrap_or("core").to_string(),
                template_key: args.optional("template").unwrap_or("").to_string(),
                importance,
                derived_from,
            })?;
            json!({ "ok": true, "command": "card-new", "card_id": card_id })
        }
        "card-move" => {
            let id = args.required_i64("id")?;
            let to = QuestionState::parse(args.required("to")?)?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let from = store.move_question_card(id, to, &trigger)?;
            let card = store.question_card(id)?;
            json!({
                "ok": true,
                "command": "card-move",
                "card_id": id,
                "from": from.as_str(),
                "to": to.as_str(),
                "used_count": card.used_count,
                "updated_at": card.updated_at,
            })
        }
        "card-list" => {
            let work = args.required_i64("work")?;
            let state = match args.optional("state") {
                None => None,
                Some(text) => Some(QuestionState::parse(text)?),
            };
            let cards = store.question_cards(work, state)?;
            json!({ "ok": true, "command": "card-list", "count": cards.len(), "cards": cards })
        }
        "card-events" => {
            let id = args.required_i64("id")?;
            let events = store.card_events(id)?;
            json!({ "ok": true, "command": "card-events", "card_id": id, "events": events })
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
