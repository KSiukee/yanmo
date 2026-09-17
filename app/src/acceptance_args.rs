// i18n-allow-file: 本模块给的是**启动参数与默认文件名**（命令行那条路上的东西），不是界面文案；
// 界面文案仍然只在 `frontend/src/locales/`（与命令行救援入口的中文同性质：那是另一个界面）。
//! 验收模式的**启动参数面**：认哪些参数、默认值是什么、一次要跑什么。
//!
//! 单独一份的理由：这一份只回答「从命令行怎么进来」（入口枚举、默认值、解析），
//! 隔壁 `acceptance.rs` 回答「进来之后做什么」（数据基准 / 库体检 / 界面冷启动）。
//! 两件事的变化理由不同——加一个入口动这里，改一次测量方式动的是那边。
//!
//! 解析的纪律照旧：**不认识的参数一律不管**（正常启动走原路），
//! 只由启动参数进入、不开任何网络通道、数据目录由参数给。

use std::path::PathBuf;

/// 这一份程序支不支持验收模式，支持到哪一档（`--version` 会把它报出去，脚本据此挡老版本）。
///
/// 1 = 最初的三个入口（`--self-test-bench` / `--self-test-ui` / `--check`）；
/// 2 = 多一个 `--mirror-sync`。**只增不改**：老脚本按 `>= 1` 判就永远安全。
pub const SUPPORTED: u32 = 2;

/// 验收模式的入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// 数据基准（不开窗口）
    Bench,
    /// 界面冷启动计时（开窗口）
    Ui,
    /// 体检一份库（会打开库；报告默认写在库旁边）——给"不丢稿"演练核对用
    Check,
    /// 镜像对账跑几轮（不开窗口；结论写成 JSON）——给长期运行的外部验收用
    Mirror,
}

/// 一次验收要跑的东西。
#[derive(Debug, Clone)]
pub struct Plan {
    pub mode: Mode,
    /// 数据目录（默认系统临时目录下的 `yanmo-acceptance`）
    pub dir: PathBuf,
    /// 报告写到哪（前缀：真正落盘时会补上版本号与时间戳）
    pub report: PathBuf,
    /// `--check` 的结果写哪个文件。
    ///
    /// **不能靠 stdout**：研墨是 GUI 子系统程序（`windows_subsystem = "windows"`），
    /// 在 cmd 里拿不到可用的标准输出——`println!` 写了也看不见（演练当场踩到）。
    /// 所以体检结果一律落文件，脚本用 `type` 打出来给人看，文件本身也留作证据。
    pub out: Option<PathBuf>,
    pub chapters: usize,
    pub chars: usize,
    /// `--mirror-sync` 跑几轮对账（默认 2：一轮写、一轮核对幂等）。
    pub rounds: usize,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            mode: Mode::Bench,
            dir: std::env::temp_dir().join("yanmo-acceptance"),
            report: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("验收报告"),
            out: None,
            chapters: 1000,
            chars: 3000,
            rounds: 2,
        }
    }
}

/// 认启动参数。不认识的参数一律不管（正常启动走原路）。
pub fn parse(argv: &[String]) -> Option<Plan> {
    let mut plan = Plan::default();
    let mut seen = false;
    for (index, arg) in argv.iter().enumerate() {
        match arg.as_str() {
            "--self-test-bench" | "--self-test" => {
                plan.mode = Mode::Bench;
                seen = true;
            }
            "--self-test-ui" => {
                plan.mode = Mode::Ui;
                seen = true;
            }
            "--check" => {
                plan.mode = Mode::Check;
                seen = true;
                if let Some(dir) = argv.get(index + 1) {
                    plan.dir = PathBuf::from(dir);
                }
            }
            "--mirror-sync" => {
                plan.mode = Mode::Mirror;
                seen = true;
                // 目录跟在后面就给（与 `--check` 同一个写法）；后面是别的开关就不吃它
                if let Some(dir) = argv.get(index + 1).filter(|raw| !raw.starts_with("--")) {
                    plan.dir = PathBuf::from(dir);
                }
            }
            "--rounds" => {
                if let Some(value) = argv.get(index + 1).and_then(|raw| raw.parse().ok()) {
                    plan.rounds = value;
                }
            }
            "--dir" => {
                if let Some(dir) = argv.get(index + 1) {
                    plan.dir = PathBuf::from(dir);
                }
            }
            "--report" => {
                if let Some(path) = argv.get(index + 1) {
                    plan.report = PathBuf::from(path);
                }
            }
            "--out" => {
                if let Some(path) = argv.get(index + 1) {
                    plan.out = Some(PathBuf::from(path));
                }
            }
            "--chapters" => {
                if let Some(value) = argv.get(index + 1).and_then(|raw| raw.parse().ok()) {
                    plan.chapters = value;
                }
            }
            "--chars" => {
                if let Some(value) = argv.get(index + 1).and_then(|raw| raw.parse().ok()) {
                    plan.chars = value;
                }
            }
            _ => {}
        }
    }
    seen.then_some(plan)
}

