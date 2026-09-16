//! 开发档里的**叩问命令面**：问题卡 / 选题 / 偏好 / 延后队列 / 处置四件套。
//!
//! 与 [`crate::dev`] 一样**只存在于开发构建**（发布版连解析分支都没有）。
//! 单独成文件的原因：这一族命令已经长得比"会话与正文"那一族还大，
//! 而且它的变化理由与它们不同（叩问在长，别把会话命令一起搅进来）。

use serde_json::{json, Value};
use yanmo_core::model::{NewQuestionCard, QuestionState};
use yanmo_core::question::{DeferCondition, DeferKind, DeferPreset, DAY_MS};
use yanmo_core::store::{RoundItem, Store};

use crate::args::{Args, Usage};
use crate::dev::body_of;
use crate::CliError;

/// 叩问这一族命令允许哪些选项。
pub fn options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "card-new" => Some(&[
            "work",
            "body",
            "body-file",
            "source",
            "template",
            "importance",
            "linked",
            "derived-from",
            "auto-derived",
        ]),
        "card-move" => Some(&["id", "to", "trigger"]),
        "card-list" => Some(&["work", "state"]),
        "card-events" => Some(&["id"]),
        // 叩问·选题与偏好：草稿 / 排序 / 学到了什么 / 说好 / 解除静音
        "question-draft" => Some(&["work"]),
        "question-select" => Some(&["work", "limit", "node"]),
        "question-weights" => Some(&[]),
        "question-praise" => Some(&["id", "trigger"]),
        "question-unmute" => Some(&["template"]),
        // 叩问·延后队列：带条件地延后 / 到条件重出 / 看还等着什么
        "question-defer" => Some(&["id", "preset", "kind", "after-days", "after-ms", "anchor-node", "note", "trigger"]),
        "question-requeue" => Some(&["work", "now-ms", "trigger"]),
        "question-deferrals" => Some(&["work", "card"]),
        // 叩问·处置：冷却库 / 捞回 / 按来源静音 / 记灵感
        "question-cooled" => Some(&["work"]),
        "question-retrieve" => Some(&["id", "trigger"]),
        "question-undefer" => Some(&["id", "trigger"]),
        "question-sources" => Some(&[]),
        "question-mute-source" => Some(&["source", "off"]),
        "question-inspire" => Some(&["id", "body", "body-file", "source", "trigger"]),
        "question-inspirations" => Some(&["id"]),
        // 叩问·作答：答案进答案池（文本与输入方式解耦：--source 记怎么打出来的）
        "question-answer" => Some(&["id", "body", "body-file", "source", "trigger"]),
        "question-answers" => Some(&["id"]),
        "question-land" => Some(&["id", "node", "trigger"]),
        // 先问后排版：一轮一次落（顺序就是 --items 里的顺序）
        "question-round" => Some(&["work", "node", "items", "items-file", "trigger"]),
        _ => None,
    }
}

/// 执行叩问这一族的命令；不是这一族就返回 `None`（交回给 [`crate::dev`]）。
pub fn execute(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    let value = match args.command.as_str() {
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
            // 关联锚点用逗号分隔（生成器给的 anchors 直接贴过来即可）
            let linked: Vec<String> = args
                .optional("linked")
                .map(|text| {
                    text.split(',')
                        .map(|item| item.trim().to_string())
                        .filter(|item| !item.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let card_id = store.create_question_card(&NewQuestionCard {
                work_id: work,
                body,
                source: args.optional("source").unwrap_or("core").to_string(),
                template_key: args.optional("template").unwrap_or("").to_string(),
                importance,
                linked,
                derived_from,
                // 这是个"在不在"的开关：给了 `--auto-derived` 就是系统自动派生的
                auto_derived: args.flag("auto-derived"),
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
        "question-draft" => {
            // 草稿里没有一个字的句子（文案在界面字典里）——驱动方按 locale 渲染，或直接看结构
            let work = args.required_i64("work")?;
            let drafts = store.question_drafts(work)?;
            json!({ "ok": true, "command": "question-draft", "count": drafts.len(), "drafts": drafts })
        }
        "question-select" => {
            let work = args.required_i64("work")?;
            let limit = match args.optional("limit") {
                None => 5usize,
                Some(text) => text
                    .parse::<usize>()
                    .map_err(|_| Usage::from("--limit 需要是一个非负整数"))?,
            };
            // 给了 --node 就是模式 A 的顺序：与这一章有关的问题排最前
            let picked = match args.optional("node") {
                Some(text) => {
                    let node = text
                        .parse::<i64>()
                        .map_err(|_| Usage::from("--node 需要是一个整数（章节节点 id）"))?;
                    store.select_questions_for_chapter(work, node, limit)?
                }
                None => store.select_questions(work, limit)?,
            };
            json!({ "ok": true, "command": "question-select", "count": picked.len(), "questions": picked })
        }
        "question-weights" => {
            let learned = store.template_weights()?;
            json!({ "ok": true, "command": "question-weights", "count": learned.len(), "weights": learned })
        }
        "question-praise" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            store.praise_question_card(id, &trigger)?;
            let key = store.question_card(id)?.template_key;
            let learned = store.template_weight(&key)?;
            json!({ "ok": true, "command": "question-praise", "card_id": id, "template_key": key, "learned": learned })
        }
        "question-unmute" => {
            let key = args.required("template")?;
            let lifted = store.unmute_template(key)?;
            json!({ "ok": true, "command": "question-unmute", "template_key": key, "learned": lifted })
        }
        "question-defer" => {
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let note = args.optional("note").unwrap_or("").to_string();
            let deferral_id = match args.optional("preset") {
                Some(key) => {
                    let preset = DeferPreset::parse(key).ok_or_else(|| {
                        Usage::from(
                            "--preset 只认 after_one_day / after_three_days / after_one_week / \
                             when_chapter_written / only_when_asked",
                        )
                    })?;
                    store.defer_question_card_by_preset(id, preset, &note, &trigger)?
                }
                None => {
                    let condition = match DeferKind::parse(args.required("kind")?)? {
                        DeferKind::Time => {
                            let due_at = match args.optional("after-ms") {
                                Some(text) => text
                                    .parse::<i64>()
                                    .map_err(|_| Usage::from("--after-ms 需要是一个整数（unix 毫秒）"))?,
                                None => {
                                    let days: i64 = args
                                        .optional("after-days")
                                        .unwrap_or("1")
                                        .parse()
                                        .map_err(|_| Usage::from("--after-days 需要是一个整数"))?;
                                    yanmo_core::time::now_millis() + days * DAY_MS
                                }
                            };
                            DeferCondition::after_ms(due_at)
                        }
                        DeferKind::Written => {
                            DeferCondition::when_written(args.required_i64("anchor-node")?)
                        }
                        DeferKind::Manual => DeferCondition::manual(),
                    };
                    store.defer_question_card(id, condition, &note, &trigger)?
                }
            };
            json!({ "ok": true, "command": "question-defer", "card_id": id, "deferral_id": deferral_id })
        }
        "question-requeue" => {
            let work = args.required_i64("work")?;
            let now_ms = match args.optional("now-ms") {
                Some(text) => text
                    .parse::<i64>()
                    .map_err(|_| Usage::from("--now-ms 需要是一个整数（unix 毫秒）"))?,
                None => yanmo_core::time::now_millis(),
            };
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let cards = store.requeue_due_questions(work, now_ms, &trigger)?;
            json!({ "ok": true, "command": "question-requeue", "count": cards.len(), "cards": cards })
        }
        "question-cooled" => {
            let work = args.required_i64("work")?;
            let cooled = store.cooled_questions(work)?;
            json!({ "ok": true, "command": "question-cooled", "count": cooled.len(), "cooled": cooled })
        }
        "question-undefer" => {
            // 别等了：取消延后、当场回候选池（并把那条还挂着的延后记录标掉）
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            store.cancel_deferral(id, &trigger)?;
            json!({ "ok": true, "command": "question-undefer", "card_id": id })
        }
        "question-retrieve" => {
            // 从冷却库捞回：状态机那条 discarded → pending 的正规出口
            let id = args.required_i64("id")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let from = store.move_question_card(id, QuestionState::Pending, &trigger)?;
            json!({ "ok": true, "command": "question-retrieve", "card_id": id, "from": from.as_str() })
        }
        "question-sources" => {
            let muted = store.muted_sources()?;
            json!({ "ok": true, "command": "question-sources", "count": muted.len(), "muted": muted })
        }
        "question-mute-source" => {
            let source = args.required("source")?;
            if args.flag("off") {
                store.unmute_source(source)?;
            } else {
                store.mute_source(source)?;
            }
            let muted = store.muted_sources()?;
            json!({ "ok": true, "command": "question-mute-source", "muted": muted })
        }
        "question-inspire" => {
            // 记灵感：不动问题状态（正交），只落一张带溯源的问题派生碎片
            let id = args.required_i64("id")?;
            let body = body_of(args)?;
            let source = args.optional("source").unwrap_or("typed").to_string();
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let idea_id = store.record_question_inspiration(id, &body, &source, &trigger)?;
            json!({ "ok": true, "command": "question-inspire", "card_id": id, "idea_id": idea_id })
        }
        "question-inspirations" => {
            let id = args.required_i64("id")?;
            let ideas = store.inspirations_of_question(id)?;
            json!({ "ok": true, "command": "question-inspirations", "card_id": id,
                    "count": ideas.len(), "inspirations": ideas })
        }
        "question-answer" => {
            // 作答：答案进答案池，卡走到「已答」终态——正文一个字节都不动
            let id = args.required_i64("id")?;
            let body = body_of(args)?;
            let source = args.optional("source").unwrap_or("typed").to_string();
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let answer_id = store.record_question_answer(id, &body, &source, &trigger)?;
            json!({ "ok": true, "command": "question-answer", "card_id": id, "answer_id": answer_id })
        }
        "question-answers" => {
            let id = args.required_i64("id")?;
            let answers = store.answers_of_question(id)?;
            json!({ "ok": true, "command": "question-answers", "card_id": id,
                    "count": answers.len(), "answers": answers })
        }
        "question-land" => {
            // 落章：**只留痕，不写正文**——正文那一段字由界面插进编辑会话（这里不动稿子）
            let id = args.required_i64("id")?;
            let node = args.required_i64("node")?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let answer_id = store.mark_answer_landed(id, node, &trigger)?;
            json!({ "ok": true, "command": "question-land", "card_id": id,
                    "answer_id": answer_id, "node_id": node })
        }
        "question-round" => {
            // 一轮落章（先问后排版）：把一串答案**一次**落进这一章。--items 给 JSON：
            // [{"card_id":1,"body":"第一段"}, …]——顺序就是落下去的顺序。
            // 正文那几段字由界面插进编辑会话，这里同样**不动稿子**，只做账。
            let work = args.required_i64("work")?;
            let node = args.required_i64("node")?;
            let raw = match args.optional("items-file") {
                Some(path) => std::fs::read_to_string(path)?,
                None => args.required("items")?.to_string(),
            };
            let items: Vec<RoundItem> = serde_json::from_str(&raw).map_err(|_| {
                Usage::from(r#"--items 需要是一个 JSON 数组，例如 [{"card_id":1,"body":"第一段"}]"#)
            })?;
            let trigger = args.optional("trigger").unwrap_or("cli").to_string();
            let landed = store.apply_answer_round(work, node, &items, &trigger)?;
            json!({ "ok": true, "command": "question-round", "count": landed.len(),
                    "answers": landed })
        }
        "question-deferrals" => {
            // 两种问法：这本书里**还等着**的（默认），或某张卡的**全部历史**
            if let Some(card) = args.optional("card") {
                let card_id = card
                    .parse::<i64>()
                    .map_err(|_| Usage::from("--card 需要是一个整数（卡 id）"))?;
                let history = store.card_deferrals(card_id)?;
                json!({ "ok": true, "command": "question-deferrals", "card_id": card_id,
                        "count": history.len(), "deferrals": history })
            } else {
                let work = args.required_i64("work")?;
                let open = store.open_deferrals(work)?;
                json!({ "ok": true, "command": "question-deferrals", "count": open.len(),
                        "deferrals": open })
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
