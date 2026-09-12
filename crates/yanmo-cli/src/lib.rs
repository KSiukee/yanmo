//! 研墨的命令行入口：**不开窗也能读、写、导出与体检**。
//!
//! 与桌面壳同一条纪律：业务全在 `yanmo-core`，这里只做「解析参数 → 调核心 → 输出 JSON」。
//! 输出一律是 JSON（成功 `{"ok":true,…}`；失败 `{"ok":false,"code":…,"params":{…}}`）——
//! 机器读得懂，人看着也不费劲，而且**不在这里拼人类句子**（界面文案属于桌面壳那一层）。
//!
//! 它存在的理由：桌面壳要人点得动；而"写到一半被杀 / 库坏了 / 盘满了"这类事
//! 得能反复跑、跑很多次——需要一个能被脚本驱动的入口。
//!
//! ```text
//! yanmo-cli --data <目录> begin
//! yanmo-cli --data <目录> write --node 3 --body "第一章的正文。"
//! yanmo-cli --data <目录> report
//! yanmo-cli --data <目录> verify
//! ```

pub mod args;
pub mod commands;

pub use commands::execute;

use std::collections::BTreeMap;

use serde_json::{json, Value};

use args::{Args, Usage};

/// 命令行失败：用法错 / 核心错 / 文件错——三者的退出码与 JSON 形状都不一样。
#[derive(Debug)]
pub enum CliError {
    Usage(Usage),
    Core(yanmo_core::Error),
    Io(std::io::Error),
}

impl From<Usage> for CliError {
    fn from(usage: Usage) -> Self {
        CliError::Usage(usage)
    }
}

impl From<yanmo_core::Error> for CliError {
    fn from(error: yanmo_core::Error) -> Self {
        CliError::Core(error)
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        CliError::Io(error)
    }
}

/// 失败的 JSON 形状：**码 + 参数**（界面能查字典，脚本能按码分支）。
fn error_json(error: &CliError) -> Value {
    match error {
        CliError::Usage(usage) => json!({ "ok": false, "code": "cli.usage", "detail": usage.0 }),
        CliError::Io(io) => json!({ "ok": false, "code": "io", "params": { "detail": io.to_string() } }),
        CliError::Core(core) => {
            let params: BTreeMap<String, String> =
                core.params().into_iter().map(|(name, value)| (name.to_string(), value)).collect();
            json!({ "ok": false, "code": core.code(), "params": params })
        }
    }
}

/// 跑一条命令（`argv` 不含程序名），返回进程退出码。
pub fn run(argv: Vec<String>) -> i32 {
    match dispatch(&argv) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(CliError::Usage(usage)) => {
            eprintln!("{}", error_json(&CliError::Usage(usage)));
            2
        }
        Err(error) => {
            // 命令失败也打在 stdout：脚本统一从 stdout 读一行 JSON 就够
            println!("{}", error_json(&error));
            1
        }
    }
}

fn dispatch(argv: &[String]) -> Result<Value, CliError> {
    if argv.iter().any(|item| item == "--help" || item == "-h") {
        return Ok(json!({ "ok": true, "usage": args::HELP }));
    }
    if argv.iter().any(|item| item == "--version" || item == "-V") {
        return Ok(json!({ "ok": true, "version": yanmo_core::version::version_string() }));
    }
    let args = Args::parse(argv)?;
    commands::execute(&args)
}
