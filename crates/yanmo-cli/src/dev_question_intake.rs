//! 开发档：**模拟一个模块提交叩问**——把「一个出口」的规则从命令行也走一遍。
//!
//! 为什么单独成件：`dev_question.rs` 已经在行数上限边上，而这一族（模块通道）
//! 与"核心自己出题/处置"的变化理由不一样——分开放，两边都不必为对方让行。
//!
//! 它只存在于开发构建（发布版连解析分支都没有）。模块地基还没落地，所以凭据由
//! 命令行直接给（`--undeclared` / `--inactive`）——验的是**数据层的规则**：
//! 未声明 / 未激活 / 冒充核心 / 没依据 / 超频，一次都进不来，且丢弃有据可查。

use serde_json::{json, Value};
use yanmo_core::model::{ModuleQuestion, ModuleQuestionGrant};
use yanmo_core::store::{QuestionIntakeOutcome, Store};

use crate::args::{Args, Usage};
use crate::dev::body_of;
use crate::CliError;

/// 这一族允许哪些选项。
pub fn options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "question-submit" => Some(&[
            "work",
            "module-id",
            "body",
            "body-file",
            "anchor",
            "importance",
            "undeclared",
            "inactive",
        ]),
        _ => None,
    }
}

/// 执行；不是这一族就返回 `None`。
pub fn execute(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    if args.command != "question-submit" {
        return Ok(None);
    }
    let work = args.required_i64("work")?;
    let module_id = args.required("module-id")?;
    let body = body_of(args)?;
    let importance = match args.optional("importance") {
        None => 0.5,
        Some(text) => text
            .parse::<f64>()
            .map_err(|_| Usage::from("--importance 需要是一个 0~1 的数"))?,
    };
    // 依据锚点：命令行一次给一条就够（要试"没依据"就别给）
    let linked: Vec<String> = args.optional("anchor").map(|anchor| vec![anchor.to_string()]).unwrap_or_default();
    let grant = ModuleQuestionGrant {
        module_id: module_id.to_string(),
        may_ask: !args.flag("undeclared"),
        active: !args.flag("inactive"),
    };
    let outcome = store.submit_module_question(
        &grant,
        ModuleQuestion { work_id: work, body, importance, linked },
    )?;
    Ok(Some(match outcome {
        QuestionIntakeOutcome::Accepted { card_id } => json!({
            "ok": true,
            "command": "question-submit",
            "outcome": "accepted",
            "module_id": module_id,
            "card_id": card_id,
        }),
        QuestionIntakeOutcome::Dropped { reason } => json!({
            "ok": true,
            "command": "question-submit",
            "outcome": "dropped",
            "module_id": module_id,
            "reason": reason.as_str(),
        }),
    }))
}
