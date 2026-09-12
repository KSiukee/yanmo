//! 命令行参数：极小的一份解析，**不引参数库**。
//!
//! 形状是固定的：`yanmo-cli --data <目录> <命令> [--键 值]…`。
//! 全局只有 `--data`（数据目录，必填）——命令行工具不猜路径，
//! 由调用方明确给出（桌面壳的默认数据目录是壳的事，见 `app/src/storage.rs`）。

use std::collections::BTreeMap;
use std::path::PathBuf;

/// 解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// 数据目录（放着 `yanmo.db`）
    pub data: PathBuf,
    /// 命令名（begin / write / read / …）
    pub command: String,
    /// 其余 `--键 值`；取值缺省时是空串
    pub options: BTreeMap<String, String>,
}

/// 用法错误（退出码 2，与"命令执行失败"区分开）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage(pub String);

impl From<&str> for Usage {
    fn from(text: &str) -> Self {
        Usage(text.to_string())
    }
}

impl From<String> for Usage {
    fn from(text: String) -> Self {
        Usage(text)
    }
}

/// 救援档帮助（**任何构建都有**）：只读 + 导出，一条都不改稿库。
pub const HELP_RESCUE: &str = "\
yanmo-cli — 研墨的命令行入口（把稿子取出来）

用法：yanmo-cli --data <数据目录> <命令> [--键 值]…

命令（全部只读或只写文件，**不会改动稿库**）：
  verify                    体检：库文件完整性 + 结构版本
  works                     列出书架上的书
  nodes --work <id>         列出一本书的目录（元数据，不含正文）
  read --node <id>          读一章正文
  export --work <id> --format txt|json --out <目录>
                            把一本书导出成文件

输出：统一 JSON（成功 {\"ok\":true,…}；失败 {\"ok\":false,\"code\":…,\"params\":{…}}）——
      命令失败打在 stdout，用法错误（参数写错）打在 stderr。
退出码：0 成功 / 1 命令失败 / 2 用法错误。";

/// 开发档帮助（**只在开发构建里有**）：给自动化与场景复现用。
pub const HELP_DEV: &str = "\
这个构建还多带这些命令（发布构建里不存在）：
  begin                     开始一次会话（登记标记），并报告上次退得干不干净
  report                    只看上次会话的交代（只读，不写标记）
  note-open --node <id>     记下「现在打开的是哪一章」
  write --node <id> --body <文本> | --body-file <路径>
                            写入一章正文（内容没变时不写库）
  fingerprint --node <id>   库里正文的指纹
  end --node <id>           正常退出收尾（留快照 + 标记干净）
  abandon                   放弃这次会话（只标记干净，不留快照）
  new-work --kind <novel|collection|article> --title <书名>
                            新建一本书
  new-node --work <id> [--parent <id>] --kind <volume|chapter|section|piece|scene> [--title <名字>]
                            在书里新建一个节点（标题留空＝按同层取号命名）
  hold --node <id> [--seconds <n>]
                            把这次会话保持打开（默认 30 秒）——供外部在「运行中」中断它";

/// 这个构建的帮助文本（发布构建只列救援档）。
pub fn help_text() -> String {
    if cfg!(debug_assertions) {
        format!("{HELP_RESCUE}\n\n{HELP_DEV}")
    } else {
        HELP_RESCUE.to_string()
    }
}

impl Args {
    /// 解析参数（`argv` 不含程序名）。
    pub fn parse(argv: &[String]) -> Result<Self, Usage> {
        let mut data: Option<PathBuf> = None;
        let mut command: Option<String> = None;
        let mut options: BTreeMap<String, String> = BTreeMap::new();
        let mut index = 0;
        while index < argv.len() {
            let token = argv[index].as_str();
            if let Some(name) = token.strip_prefix("--") {
                // `--key value`：下一个 token 不是 `--` 开头就当取值；否则视为无值开关
                let value = match argv.get(index + 1) {
                    Some(next) if !next.starts_with("--") => {
                        index += 1;
                        next.clone()
                    }
                    _ => String::new(),
                };
                match name {
                    "data" => {
                        if value.is_empty() {
                            return Err(Usage::from("--data 后面要给一个目录"));
                        }
                        data = Some(PathBuf::from(value));
                    }
                    _ => {
                        options.insert(name.to_string(), value);
                    }
                }
            } else if command.is_none() {
                command = Some(token.to_string());
            } else {
                return Err(Usage(format!("多余的参数：{token}")));
            }
            index += 1;
        }
        let command = command.ok_or_else(|| Usage::from("没有给命令（例如 begin / write / read）"))?;
        let data = data.ok_or_else(|| Usage::from("缺少 --data <数据目录>"))?;
        Ok(Args { data, command, options })
    }

    /// 取一个必填选项。
    pub fn required(&self, name: &str) -> Result<&str, Usage> {
        self.options
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Usage(format!("命令 {} 需要 --{name}", self.command)))
    }

    /// 取一个必填的数字选项。
    pub fn required_i64(&self, name: &str) -> Result<i64, Usage> {
        self.required(name)?
            .parse::<i64>()
            .map_err(|_| Usage(format!("--{name} 需要是一个整数")))
    }

    /// 取一个可选选项。
    pub fn optional(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str).filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn parses_data_command_and_options() {
        let args = Args::parse(&argv(&["--data", "/tmp/x", "write", "--node", "7", "--body", "字"])).unwrap();
        assert_eq!(args.data, PathBuf::from("/tmp/x"));
        assert_eq!(args.command, "write");
        assert_eq!(args.required_i64("node").unwrap(), 7);
        assert_eq!(args.required("body").unwrap(), "字");
    }

    #[test]
    fn data_and_command_may_come_in_any_order() {
        let args = Args::parse(&argv(&["begin", "--data", "/tmp/x"])).unwrap();
        assert_eq!(args.command, "begin");
        assert_eq!(args.data, PathBuf::from("/tmp/x"));
    }

    #[test]
    fn missing_data_or_command_is_a_usage_error() {
        assert!(Args::parse(&argv(&["begin"])).is_err(), "缺 --data 应报用法错误");
        assert!(Args::parse(&argv(&["--data", "/tmp/x"])).is_err(), "缺命令应报用法错误");
    }

    #[test]
    fn a_valueless_flag_does_not_swallow_the_next_option() {
        let args = Args::parse(&argv(&["--data", "/tmp/x", "abandon", "--quiet", "--node", "3"])).unwrap();
        assert_eq!(args.optional("quiet"), None);
        assert_eq!(args.required_i64("node").unwrap(), 3, "--quiet 不该吞掉后面的 --node");
    }

    #[test]
    fn a_missing_required_option_is_a_usage_error() {
        let args = Args::parse(&argv(&["--data", "/tmp/x", "write"])).unwrap();
        assert!(args.required("node").is_err());
    }
}
