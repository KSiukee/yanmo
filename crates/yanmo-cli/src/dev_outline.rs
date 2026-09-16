//! 开发档里的**大纲命令面**：设定卡（人物 / 设定）、场景卡四格、大纲体检。
//!
//! 与别的开发档命令一样**只存在于开发构建**。单独成文件的原因：
//! 它是同一件事的三个面（先有承载，才判得动，判完还能处置）——
//! 界面上要点得动，而"重名会不会报""忽略记没记住""软删的还报不报"这类事
//! 得能反复跑、跑很多次，界面做不成自动化。
//!
//! 锚点与参数的写法与界面完全一致（`entity:3` / `scene:9`），所以命令行扫出来的
//! 那份清单能直接喂给 `outline-dismiss` 验证"忽略"这条路。

use serde_json::{json, Value};
use yanmo_core::model::{
    Attribute, EntityKind, ForeshadowState, NewEntityCard, NewForeshadow, SceneFields,
};
use yanmo_core::store::{FragmentEdit, Store};

use crate::args::{Args, Usage};
use crate::dev::body_of;
use crate::CliError;

/// 大纲这一族命令允许哪些选项。
pub fn options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "entity-new" => Some(&["work", "kind", "name", "alias", "attr", "note", "trigger"]),
        "entity-list" => Some(&["work", "kind"]),
        "entity-update" => Some(&["id", "kind", "name", "alias", "attr", "note", "trigger"]),
        "entity-delete" => Some(&["id", "trigger"]),
        "scene-field" => Some(&["node", "pov", "goal", "conflict", "outcome", "trigger"]),
        "foreshadow-new" => Some(&["work", "body", "body-file", "node", "note", "trigger"]),
        "foreshadow-list" => Some(&["work", "state"]),
        "foreshadow-update" => Some(&["id", "body", "body-file", "node", "note", "trigger"]),
        "foreshadow-move" => Some(&["id", "to", "collected-node", "trigger"]),
        "foreshadow-delete" => Some(&["id", "trigger"]),
        "fragment-edit" => Some(&["id", "body", "body-file", "story-time", "story-order", "flashback", "trigger"]),
        "outline-scan" => Some(&["work"]),
        "outline-dismiss" => Some(&["work", "fingerprint", "trigger"]),
        "outline-undismiss" => Some(&["work", "fingerprint", "trigger"]),
        "outline-clear-dismissed" => Some(&["work", "trigger"]),
        _ => None,
    }
}

/// 逗号分隔的一串（`--alias 阿文,陆大人`）：空段丢掉。
fn split_list(args: &Args, name: &str) -> Vec<String> {
    args.optional(name)
        .map(|text| {
            text.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `--attr 发色=黑,佩剑=青霜` → 一串键值（没写 `=` 的算"只有键"，与界面一致）。
fn attributes_of(args: &Args) -> Vec<Attribute> {
    split_list(args, "attr")
        .into_iter()
        .map(|item| match item.split_once('=') {
            Some((key, value)) => Attribute {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            },
            None => Attribute { key: item, value: String::new() },
        })
        .collect()
}

/// 表单那一份（新建与整卡覆盖共用）。
fn draft_of(args: &Args, work_id: i64) -> Result<NewEntityCard, CliError> {
    Ok(NewEntityCard {
        work_id,
        kind: EntityKind::parse(args.required("kind")?)?,
        name: args.required("name")?.to_string(),
        aliases: split_list(args, "alias"),
        attributes: attributes_of(args),
        note: args.optional("note").unwrap_or("").to_string(),
    })
}

fn fields_of(args: &Args) -> Result<SceneFields, CliError> {
    let node_id = args.required_i64("node")?;
    let get = |name: &str| args.optional(name).unwrap_or("").to_string();
    Ok(SceneFields {
        node_id,
        pov: get("pov"),
        goal: get("goal"),
        conflict: get("conflict"),
        outcome: get("outcome"),
    })
}

/// 执行大纲这一族的命令；不是这一族就返回 `None`（交回给别的族）。
pub fn execute(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    let value = match args.command.as_str() {
        "entity-new" => {
            let work = args.required_i64("work")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let id = store.create_entity_card(&draft_of(args, work)?, &trigger)?;
            json!({ "ok": true, "command": "entity-new", "card": store.entity_card(id)? })
        }
        "entity-list" => {
            let work = args.required_i64("work")?;
            let kind = match args.optional("kind") {
                Some(code) => Some(EntityKind::parse(code)?),
                None => None,
            };
            let cards = store.entity_cards(work, kind)?;
            json!({ "ok": true, "command": "entity-list", "count": cards.len(), "cards": cards })
        }
        "entity-update" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            // 这本书是哪一本由卡自己说了算（命令行也不必再报一遍）
            let work = store.entity_card(id)?.work_id;
            store.update_entity_card(id, &draft_of(args, work)?, &trigger)?;
            json!({ "ok": true, "command": "entity-update", "card": store.entity_card(id)? })
        }
        "entity-delete" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let gone = store.delete_entity_card(id, &trigger)?;
            json!({ "ok": true, "command": "entity-delete", "entity_id": gone.id,
                    "name": gone.name })
        }
        "scene-field" => {
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let saved = store.save_scene_fields(&fields_of(args)?, &trigger)?;
            json!({ "ok": true, "command": "scene-field", "fields": saved })
        }
        "foreshadow-new" => {
            let work = args.required_i64("work")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let planted = match args.optional("node") {
                Some(text) => Some(text.parse::<i64>().map_err(|_| Usage::from("--node 需要是一个整数"))?),
                None => None,
            };
            let id = store.create_foreshadow(
                &NewForeshadow {
                    work_id: work,
                    body: body_of(args)?,
                    planted_node: planted,
                    note: args.optional("note").unwrap_or("").to_string(),
                },
                &trigger,
            )?;
            json!({ "ok": true, "command": "foreshadow-new", "item": store.foreshadow(id)? })
        }
        "foreshadow-list" => {
            let work = args.required_i64("work")?;
            let state = match args.optional("state") {
                Some(code) => Some(ForeshadowState::parse(code)?),
                None => None,
            };
            let items = store.foreshadows(work, state)?;
            json!({ "ok": true, "command": "foreshadow-list", "count": items.len(), "items": items })
        }
        "foreshadow-update" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let planted = match args.optional("node") {
                Some(text) => Some(text.parse::<i64>().map_err(|_| Usage::from("--node 需要是一个整数"))?),
                None => None,
            };
            let item = store.update_foreshadow(
                id,
                &body_of(args)?,
                planted,
                args.optional("note").unwrap_or(""),
                &trigger,
            )?;
            json!({ "ok": true, "command": "foreshadow-update", "item": item })
        }
        "foreshadow-move" => {
            let id = args.required_i64("id")?;
            let to = ForeshadowState::parse(args.required("to")?)?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let collected = match args.optional("collected-node") {
                Some(text) => {
                    Some(text.parse::<i64>().map_err(|_| Usage::from("--collected-node 需要是一个整数"))?)
                }
                None => None,
            };
            let item = store.move_foreshadow(id, to, collected, &trigger)?;
            json!({ "ok": true, "command": "foreshadow-move", "item": item })
        }
        "foreshadow-delete" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let gone = store.delete_foreshadow(id, &trigger)?;
            json!({ "ok": true, "command": "foreshadow-delete", "foreshadow_id": gone.id })
        }
        "fragment-edit" => {
            // 正文 + 故事时间（只有事件用得上）：一次给全，与界面那张小表单同形
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let order = match args.optional("story-order") {
                Some(text) => Some(text.parse::<i64>().map_err(|_| Usage::from("--story-order 需要是一个整数"))?),
                None => None,
            };
            let saved = store.update_fragment(
                &FragmentEdit {
                    id,
                    body: body_of(args)?,
                    story_time: args.optional("story-time").unwrap_or("").to_string(),
                    story_order: order,
                    flashback: args.flag("flashback"),
                },
                &trigger,
            )?;
            json!({ "ok": true, "command": "fragment-edit", "fragment": saved })
        }
        "outline-scan" => {
            let work = args.required_i64("work")?;
            let issues = store.outline_issues(work)?;
            // 与界面同一份形状：**带指纹**（忽略认它）
            let shaped: Vec<Value> = issues
                .iter()
                .map(|issue| {
                    json!({
                        "rule": issue.rule.as_str(),
                        "anchors": issue.anchors,
                        "params": issue.params,
                        "fingerprint": issue.fingerprint(),
                    })
                })
                .collect();
            json!({ "ok": true, "command": "outline-scan", "count": shaped.len(),
                    "issues": shaped, "dismissed": store.dismissed_issues(work)? })
        }
        "outline-dismiss" => {
            let work = args.required_i64("work")?;
            let fingerprint = args.required("fingerprint")?.to_string();
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            store.dismiss_issue(work, &fingerprint, &trigger)?;
            json!({ "ok": true, "command": "outline-dismiss",
                    "dismissed": store.dismissed_issues(work)? })
        }
        "outline-undismiss" => {
            let work = args.required_i64("work")?;
            let fingerprint = args.required("fingerprint")?.to_string();
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            store.undismiss_issue(work, &fingerprint, &trigger)?;
            json!({ "ok": true, "command": "outline-undismiss",
                    "dismissed": store.dismissed_issues(work)? })
        }
        "outline-clear-dismissed" => {
            let work = args.required_i64("work")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            store.clear_dismissed_issues(work, &trigger)?;
            json!({ "ok": true, "command": "outline-clear-dismissed", "dismissed": Vec::<String>::new() })
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
