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

/// 救援档帮助（**任何构建都有**）：看稿那几条 + 一条"默认只看不写"的导入。
///
/// 措辞注意（2026-09-15 代码质量评审：中等 4）：看稿这几条走的是 `Store::open`，
/// 会**打开**库（结构迁移 / 本机登记 / 缺标记时回填字数）——不许再说成"不会改动稿库"。
/// 它们改的从来不是作者的稿子（正文一个字都不动），但会动库文件本身。
pub const HELP_RESCUE: &str = "\
yanmo-cli — 研墨的命令行入口

**不带命令直接运行** = 打开中文菜单（给作者的备用导出工具：看稿 / 体检 / 导出 / 退出）。
图形界面打不开的时候（显卡、驱动、界面本身出问题），双击本程序即可，不用记任何参数。

菜单模式另认两个可选参数：
  --data <目录>        指定稿子库所在目录（不给就按系统约定自动找）
  --export-to <目录>   把稿子导到这里（想导到 U 盘时用；不给就放「文档」下）

想脚本化时按下面的用法给命令：

用法：yanmo-cli --data <数据目录> <命令> [--键 值]…

命令一：**看稿**（正文一个字都不改；但会打开库）：
  这几条都会打开库：可能升级结构、登记这台机器、给老库回填字数标记——
  动的是库文件本身，**不是你的稿子**。要体检一份**坏库**：先把库文件复制一份再跑。
  verify                    体检：库文件完整性 + 结构版本
  works                     列出书架上的书
  nodes --work <id>         列出一本书的目录（元数据，不含正文）
  read --node <id>          读一章正文
  search --query <文字> [--work <id>] [--limit <n>]
                            在稿子里检索（全文索引；--work 只搜一本书，--limit 默认 50、上限 500）
  export --work <id> --format txt|json --out <目录>
                            把一本书导出成文件

命令二：**从成稿导入**（库坏了、只剩备份包时的最后一条回家路）：
  import --from <备份包目录 或 work.json> [--work <书名>] [--language zh|en|ja] [--yes]
    读备份包里的「成稿/<书名>/work.json」（不碰快照、不碰数据库），把整本书**新建**回来。
    · 不给 --yes 就**只算不写**：先看清这一份里有几章、多少字、与备份清单对不对得上；
    · 给了 --yes 才写库，而且**只新建**：既有作品一个字都不动；
    · 成稿里没有的东西（每章一句话、版本历史、写作时间）不补、不编——那些行不计入每日码字；
    · 一份成稿读不进来就整批停下（不许救一半），报错里会写清是哪一格不合格。
    · 前提：这条命令写的是 --data 指的那个库。库文件坏到打不开时，先让研墨开一个新库
      （把坏掉的库文件**挪开别删**），再跑它。

输出：统一 JSON（成功 {\"ok\":true,…}；失败 {\"ok\":false,\"code\":…,\"params\":{…}}）——
      命令失败打在 stdout，用法错误（参数写错）打在 stderr。
退出码：0 成功 / 1 命令失败 / 2 用法错误。";

/// 开发档帮助（**只在开发构建里有**）：给自动化与场景复现用。
pub const HELP_DEV: &str = "\
这个构建还多带这些命令（发布构建里不存在）：
  begin                     开始一次会话（登记标记），并报告上次退得干不干净
  report                    只看上次会话的交代（只读，不写标记）
  note-open --node <id>     记下「现在打开的是哪一章」
  write --node <id> --body <文本> | --body-file <路径> [--tz <分钟>]
                            写入一章正文（内容没变时不写库）；给了 --tz 就走编辑器那条路，
                            顺带记进「每日码字」账本（东八区 480）
  fingerprint --node <id>   库里正文的指纹
  end --node <id>           正常退出收尾（留快照 + 标记干净）
  abandon                   放弃这次会话（只标记干净，不留快照）
  backup --to <目录> [--keep <n>] [--tz <分钟>] [--device <名字>]
                            立刻做一次备份（演练用：验「备份目标不可写会怎样」与保留滚动）
  new-work --kind <novel|collection|article> --title <书名>
                            新建一本书
  new-node --work <id> [--parent <id>] --kind <volume|chapter|section|piece|scene> [--title <名字>]
                            在书里新建一个节点（标题留空＝按同层取号命名）
  hold --node <id> [--seconds <n>]
                            把这次会话保持打开（默认 30 秒）——供外部在「运行中」中断它
  card-new --work <id> --body <文本> | --body-file <路径> [--source <来源>] [--template <模板键>]
           [--importance <0~1>] [--linked <锚点,锚点>] [--derived-from <卡 id>] [--auto-derived]
                            建一张问题卡（叩问）：落进碎片统一表，状态从 pending 开始；
                            --linked 是关联锚点（生成器给的那一串，去重靠它）
  card-move --id <id> --to <pending|asked|answered|deferred|discarded|muted> [--trigger <谁触发>]
                            迁移一张问题卡的状态（非法边当场拒绝；每次迁移留一条可核对的事件）
  card-list --work <id> [--state <态>]
                            列出一本书里的问题卡（不给 --state 就是全部）
  card-events --id <id>     一张卡的状态迁移史（谁触发、从哪个态到哪个态）
  question-draft --work <id>
                            按书的现状生成候选问题**草稿**（模板 + 槽位，不含句子）
  question-select --work <id> [--limit <n>]
                            按引力排出此刻该问的问题（只读；默认 5 条）
  question-weights          学到的模板权重一览（偏好学习闭环的账本）
  question-praise --id <id> [--trigger <谁触发>]
                            说「这个问题好」：状态不动，只教同类模板
  question-unmute --template <模板键>
                            解除某一类的静音（静音可撤销，绝不默认开启）
  question-defer --id <id> --preset <档位> [--note <作者填的一句>] [--trigger <谁触发>]
                 或 --id <id> --kind <time|written|manual> [--after-days <n> | --after-ms <毫秒>]
                    [--anchor-node <id>] [--note <…>]
                            带条件地延后一张卡；--preset 的档位见界面字典 question.defer.*
  question-requeue --work <id> [--now-ms <毫秒>] [--trigger <谁触发>]
                            把条件已满足的延后放回候选池（不给 --now-ms 就用此刻）
  question-deferrals --work <id> | --card <id>
                            还等着的延后（条件是什么、作者填了什么）；给 --card 看某张卡的全部延后史
  question-cooled --work <id>
                            冷却库：这本书里舍弃过的卡（最近舍弃的在前）
  question-retrieve --id <id> [--trigger <谁触发>]
                            从冷却库捞回一张卡（舍弃不真删）
  question-undefer --id <id> [--trigger <谁触发>]
                            「别等了」：取消延后、当场回候选池
  question-sources          已经静音的来源一览
  question-mute-source --source <来源> [--off]
                            按来源静音 / 解除静音（某个模块太吵时只让它闭嘴）
  question-inspire --id <卡 id> --body <文本> | --body-file <路径> [--source typed|voice|mixed]
                   [--trigger <谁触发>]
                            记一条灵感：从这张卡勾出来，**不动它的状态**
  question-inspirations --id <卡 id>
                            这张卡勾出过哪些灵感";

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

    /// **开关**：只问"给没给"，不看取值。
    ///
    /// 专门补的一条：`optional()` 会把空串当成"没给"（那对"取值型"选项是对的），
    /// 于是 `--off` / `--auto-derived` 这类**无值开关**用它永远读不到——
    /// 2026-09-15 落地延后队列时当场发现（`--off` 静默失效）。开关一律走这里。
    pub fn flag(&self, name: &str) -> bool {
        self.options.contains_key(name)
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
