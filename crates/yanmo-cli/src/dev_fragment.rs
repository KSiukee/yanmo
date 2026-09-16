//! 开发档里的**创作流命令面**：碎片池的记 / 看 / 删 / 捞回。
//!
//! 与 [`crate::dev`]、[`crate::dev_question`] 一样**只存在于开发构建**。
//! 单独成文件的原因与那一族一样：它的变化理由不同（创作流在长，别把叩问搅进来）。
//!
//! 为什么这族也要有命令行：界面上"记一条"要点得动，而"删了能捞回""坏种类会被拒"
//! 这类事得能反复跑、跑很多次——界面做不成自动化。

use serde_json::{json, Value};
use yanmo_core::model::FragmentKind;
use yanmo_core::store::{NewFragment, Store, FRAGMENTS_PER_BOARD};

use crate::args::{Args, Usage};
use crate::dev::body_of;
use crate::CliError;

/// 创作流这一族命令允许哪些选项。
pub fn options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "fragment-add" => Some(&[
            "work",
            "kind",
            "body",
            "body-file",
            "source",
            "anchor",
            "trigger",
        ]),
        "fragment-board" => Some(&["work", "limit"]),
        "fragment-delete" => Some(&["id", "trigger"]),
        "fragment-restore" => Some(&["id", "trigger"]),
        _ => None,
    }
}

/// 锚点：`--anchor chapter:12` 或 `--anchor chapter:12,chapter:13`（逗号分隔）。
///
/// 命令行没有"可重复选项"那一套，所以多个锚点挤在一个开关里——空段直接丢掉，
/// 免得 `a,,b` 这种手滑变成两个有效锚点加一个空锚点。
fn anchors_of(args: &Args) -> Vec<String> {
    args.optional("anchor")
        .map(|text| {
            text.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 执行创作流这一族的命令；不是这一族就返回 `None`（交回给别的族）。
pub fn execute(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    let value = match args.command.as_str() {
        "fragment-add" => {
            let work = args.required_i64("work")?;
            let kind = FragmentKind::parse(args.required("kind")?)?;
            let source = args.optional("source").unwrap_or("typed").to_string();
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let id = store.create_fragment(
                &NewFragment {
                    work_id: work,
                    kind,
                    body: body_of(args)?,
                    source,
                    anchors: anchors_of(args),
                },
                &trigger,
            )?;
            // 回执给**库里真有的那一条**（修剪过的原文 + 核心认下的输入方式）
            json!({ "ok": true, "command": "fragment-add", "fragment": store.fragment(id)? })
        }
        "fragment-board" => {
            let work = args.required_i64("work")?;
            let limit = match args.optional("limit") {
                Some(text) => text
                    .parse::<usize>()
                    .map_err(|_| Usage::from("--limit 需要是一个非负整数"))?,
                None => FRAGMENTS_PER_BOARD,
            };
            let kinds = FragmentKind::JOTTED;
            json!({ "ok": true, "command": "fragment-board",
                    "fragments": store.fragments(work, &kinds, limit)?,
                    "counts": store.fragment_counts(work, &kinds)? })
        }
        "fragment-delete" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let gone = store.delete_fragment(id, &trigger)?;
            json!({ "ok": true, "command": "fragment-delete", "fragment_id": gone.id,
                    "kind": gone.kind.as_str() })
        }
        "fragment-restore" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let back = store.restore_fragment(id, &trigger)?;
            json!({ "ok": true, "command": "fragment-restore", "fragment_id": back.id,
                    "kind": back.kind.as_str() })
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
