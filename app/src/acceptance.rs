// i18n-allow-file: 本模块产出的是**验收报告文件**（给人看的实测数字与表格），不是界面文案；
// 界面文案仍然只在 `frontend/src/locales/`（与命令行救援入口的中文同性质：那是另一个界面）。
//! 验收模式：把「百万字不卡」变成**任何人都能自己跑一遍**的东西。
//!
//! # 为什么做在程序里
//!
//! 一个外部脚本要装这装那，或者得动内部压测工具（那些按纪律不出门）。做进程序里，
//! 作者只要双击一个 `.bat`：造一份 300 万字的稿库、量一轮、把报告写在旁边。
//!
//! # 四条纪律
//!
//! 1. **不碰真实稿库**：数据目录由参数给（默认系统临时目录），**位置记录一个字节都不动**；
//!    你在验收模式里做什么，都不影响双击图标打开的那份稿子。
//! 2. **不开任何网络通道**：纯本机读写。
//! 3. **只由启动参数进入**：不带参数启动的行为一字不变。
//! 4. 报告是**给人看的文件**，不是界面文案。
//!
//! # 两个入口（由 `tools/acceptance/run-acceptance.bat` 依次调用）
//!
//! - `--self-test-bench`：**不开窗口**，造数据 + 量核心操作（写入/检索/读章/重开/快照/体检）；
//! - `--self-test-ui`：开窗口打开同一份库，量**冷启动到界面就绪**，合并成最终报告。
//!
//! 分两个进程是有意的：第二个进程的"冷启动"才是真的冷启动（同一个进程里先跑完基准，
//! 程序文件早进了系统缓存，量出来的数字会偏乐观）。
//!
//! 另有 `--check <目录>`：只读体检一份库，打印 JSON（不写任何东西）——"不丢稿"演练用它核对。

use std::path::{Path, PathBuf};
use std::time::Instant;

use yanmo_core::db;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{BackupRequest, BackupTarget, Store};

/// 一章的正文：一小段可复现的话反复拼——**同一份数据，谁跑都一样**。
const SENTENCE: &str = "雨下了整夜，屋檐上的水声一直没停。她把灯芯挑亮了一点，又低头写下去。";

/// 验收模式的两个入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// 数据基准（不开窗口）
    Bench,
    /// 界面冷启动计时（开窗口）
    Ui,
    /// 只读体检一份库（给"不丢稿"演练核对用）
    Check,
}

/// 一次验收要跑的东西。
#[derive(Debug, Clone)]
pub struct Plan {
    pub mode: Mode,
    /// 数据目录（默认系统临时目录下的 `yanmo-acceptance`）
    pub dir: PathBuf,
    /// 报告写到哪（前缀：真正落盘时会补上版本号与时间戳）
    pub report: PathBuf,
    pub chapters: usize,
    pub chars: usize,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            mode: Mode::Bench,
            dir: std::env::temp_dir().join("yanmo-acceptance"),
            report: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("验收报告"),
            chapters: 1000,
            chars: 3000,
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

/// 一条量出来的指标。
#[derive(Debug, Clone)]
pub struct Step {
    pub name: String,
    pub ms: f64,
    /// 附注（规模、命中数、失败原因……）
    pub note: String,
}

/// 一份验收报告。
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub steps: Vec<Step>,
    pub machine: Vec<(String, String)>,
    pub chapters: usize,
    pub chars: usize,
    pub db_bytes: u64,
}

impl Report {
    /// 量一步。`op` 失败不中断整轮——报告里如实写"这步没成"。
    fn time<T>(&mut self, name: &str, op: impl FnOnce() -> Result<(T, String), String>) -> Option<T> {
        let started = Instant::now();
        let outcome = op();
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let (value, note) = match outcome {
            Ok((value, note)) => (Some(value), note),
            Err(reason) => (None, reason),
        };
        self.steps.push(Step { name: name.to_string(), ms, note });
        value
    }
}

/// 一节正文，凑到大约 `chars` 个字。
fn body_of(chars: usize) -> String {
    let unit = SENTENCE.chars().count().max(1);
    let times = chars.div_ceil(unit);
    let mut text = SENTENCE.repeat(times);
    text.truncate(text.char_indices().nth(chars).map(|(index, _)| index).unwrap_or(text.len()));
    text
}

/// 造数据 + 量核心操作（**不开窗口**）。返回报告。
pub fn run_bench(plan: &Plan) -> Report {
    let mut report = Report { machine: machine(), ..Report::default() };
    let started = Instant::now();
    // 每次都从干净目录开始：上一次的结果不该混进这一次的数字
    let _ = std::fs::remove_dir_all(&plan.dir);
    if let Err(error) = std::fs::create_dir_all(&plan.dir) {
        report.steps.push(Step {
            name: "造数据".to_string(),
            ms: 0.0,
            note: format!("建目录失败：{error}"),
        });
        return report;
    }

    let db_path = plan.dir.join(yanmo_core::paths::DB_FILE);
    let mut store = match Store::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            report.steps.push(Step { name: "造数据".to_string(), ms: 0.0, note: format!("开库失败：{error}") });
            return report;
        }
    };

    let body = body_of(plan.chars);
    let chapters = plan.chapters;
    report.chapters = chapters;
    report.chars = body.chars().count() * chapters;

    // ① 造一本 N 章的书（这也是"写入"的真实体感：1000 章一次性铺进去）
    let work_id = report.time("造数据·写入 N 章", || {
        let work = store.create_work(WorkKind::Novel, "验收用长篇").map_err(|e| e.to_string())?;
        let volume = store
            .list_nodes(work.id)
            .map_err(|e| e.to_string())?
            .first()
            .map(|node| node.id)
            .ok_or_else(|| "新作品没有默认卷".to_string())?;
        for index in 1..=chapters {
            let chapter = store
                .create_node(work.id, Some(volume), NodeKind::Chapter, &format!("第 {index} 章"))
                .map_err(|e| e.to_string())?;
            store.write_body(chapter, &body).map_err(|e| e.to_string())?;
        }
        Ok((work.id, format!("{chapters} 章 × 约 {} 字", body.chars().count())))
    });

    // ② 全树（目录树一次拉完）
    report.time("目录树·一次全拉", || {
        let nodes = store.list_nodes(work_id.unwrap_or(1)).map_err(|e| e.to_string())?;
        Ok((nodes.len(), format!("{} 个节点", nodes.len())))
    });

    // ③ 读一章（中间那一章，避开缓存最热的首章）
    let middle = report.time("读一章正文", || {
        let nodes = store.list_nodes(work_id.unwrap_or(1)).map_err(|e| e.to_string())?;
        let target = nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Chapter)
            .nth(chapters / 2)
            .map(|node| node.id)
            .ok_or_else(|| "没有找到中间那一章".to_string())?;
        let text = store.read_body(target).map_err(|e| e.to_string())?;
        Ok((target, format!("{} 字", text.chars().count())))
    });

    // ④ 写一章（改一个字再写回去）
    if let Some(target) = middle {
        report.time("写一章正文", || {
            let text = store.read_body(target).map_err(|e| e.to_string())?;
            store.write_body(target, &format!("{text}改。")).map_err(|e| e.to_string())?;
            Ok(((), format!("{} 字", text.chars().count() + 2)))
        });
    }

    // ⑤ 检索三种形态：四个字（走全文索引）/ 两个字（回退全扫）/ 一个字（回退全扫）
    for query in ["屋檐上的", "水声", "雨"] {
        let label = format!("检索·{}（{} 字）", query, query.chars().count());
        report.time(&label, || {
            let hits = store.search(query, None, 50).map_err(|e| e.to_string())?;
            Ok((hits.len(), format!("命中 {} 条（上限 50）", hits.len())))
        });
    }

    // ⑥ 库体检（证明这份库是好的，不是一堆烂数据）
    report.time("库体检·quick_check", || {
        let result = db::quick_check(store.conn()).map_err(|e| e.to_string())?;
        Ok(((), result))
    });

    // ⑦ 一致性快照（备份一次：大库备份要多久，也是"不丢稿"的关键数字）
    let backup_to = plan.dir.join("..").join("yanmo-acceptance-backup");
    report.time("一致性快照·备份一次", || {
        let request = BackupRequest {
            data_dir: plan.dir.clone(),
            targets: vec![BackupTarget {
                path: backup_to.to_string_lossy().to_string(),
                volume_id: "acceptance".to_string(),
                volume_label: "验收用".to_string(),
                removable: false,
            }],
            keep: 1,
            tz_offset_minutes: 480,
            device: "验收模式".to_string(),
        };
        let outcome = store.backup_now(&request).map_err(|e| e.to_string())?;
        Ok(((), format!("成功 {} 个位置", outcome.succeeded())))
    });
    let _ = std::fs::remove_dir_all(&backup_to);

    // ⑧ 关库重开（真实场景：下次打开软件）。关掉的连接在这里就收干净，后面不再用它。
    let reopen_started = Instant::now();
    drop(store);
    let reopened = match Store::open(&db_path) {
        Ok(again) => again.list_nodes(work_id.unwrap_or(1)).map(|nodes| nodes.len()),
        Err(error) => {
            report.steps.push(Step {
                name: "关库再打开".to_string(),
                ms: reopen_started.elapsed().as_secs_f64() * 1000.0,
                note: format!("重开失败：{error}"),
            });
            report.db_bytes = std::fs::metadata(&db_path).map(|meta| meta.len()).unwrap_or(0);
            return report;
        }
    };
    report.steps.push(Step {
        name: "关库再打开".to_string(),
        ms: reopen_started.elapsed().as_secs_f64() * 1000.0,
        note: format!("重开后仍有 {} 个节点", reopened.unwrap_or(0)),
    });

    report.db_bytes = std::fs::metadata(&db_path).map(|meta| meta.len()).unwrap_or(0);
    report.steps.push(Step {
        name: "整轮".to_string(),
        ms: started.elapsed().as_secs_f64() * 1000.0,
        note: format!("库 {}", human_bytes(report.db_bytes)),
    });
    report
}

/// 只读体检一份库（给"不丢稿"演练核对用）：打印 JSON，**不写任何东西**。
pub fn run_check(plan: &Plan) -> i32 {
    let db_path = plan.dir.join(yanmo_core::paths::DB_FILE);
    let store = match Store::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            // i18n-allow-next-line: 命令行的机器可读输出（给脚本看），不是界面文案
            println!("{{\"ok\":false,\"error\":\"打不开库：{error}\"}}");
            return 2;
        }
    };
    let integrity = db::quick_check(store.conn()).unwrap_or_else(|error| error.to_string());
    let works = store.list_works().map(|list| list.len()).unwrap_or(0);
    let shelf = store.shelf().map(|list| list.len()).unwrap_or(0);
    let mut chapters = 0i64;
    let mut words = 0i64;
    for entry in store.shelf().unwrap_or_default() {
        chapters += entry.chapters;
        words += entry.word_count;
    }
    println!(
        "{{\"ok\":true,\"integrity\":\"{integrity}\",\"works\":{works},\"shelf\":{shelf},\"chapters\":{chapters},\"words\":{words}}}"
    );
    0
}

/// 这台机器是谁（报告里必须有——同一份脚本在低配机和高配机上数字不一样，得说得清）。
fn machine() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for (label, key) in [
        ("CPU", "PROCESSOR_IDENTIFIER"),
        ("逻辑核心", "NUMBER_OF_PROCESSORS"),
        ("系统", "OS"),
    ] {
        let value = std::env::var(key).unwrap_or_else(|_| "（问不到）".to_string());
        rows.push((label.to_string(), value));
    }
    rows.push(("内存".to_string(), total_memory()));
    rows
}

/// 物理内存（Windows 问系统；别处给"问不到"，不编一个数）。
#[cfg(windows)]
fn total_memory() -> String {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return "（问不到）".to_string();
    }
    format!("{:.1} GB", status.ullTotalPhys as f64 / 1024.0 / 1024.0 / 1024.0)
}

#[cfg(not(windows))]
fn total_memory() -> String {
    "（只在 Windows 上问得到）".to_string()
}

/// 报告落盘（`<前缀>-<版本>-<时间戳>.json` 与 `.md`），返回写出来的文件。
pub fn write_report(report: &Report, prefix: &Path) -> std::io::Result<Vec<PathBuf>> {
    let stamp = yanmo_core::time::local_stamp(
        yanmo_core::time::now_millis(),
        480,
    );
    let dir = prefix.parent().filter(|path| !path.as_os_str().is_empty());
    let stem = prefix.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "验收报告".to_string());
    let base = match dir {
        Some(dir) => dir.join(format!("{stem}-{}-{stamp}", yanmo_core::engine_version())),
        None => PathBuf::from(format!("{stem}-{}-{stamp}", yanmo_core::engine_version())),
    };
    // ⚠️ 不能用 `with_extension`：文件名里有版本号（0.27.1），
    // 它会把最后一个点后面的东西当扩展名，写出 `报告-0.27.json` 这种东西。
    let json_path = PathBuf::from(format!("{}.json", base.display()));
    let md_path = PathBuf::from(format!("{}.md", base.display()));
    std::fs::write(&json_path, to_json(report))?;
    std::fs::write(&md_path, to_markdown(report))?;
    Ok(vec![json_path, md_path])
}

fn to_json(report: &Report) -> String {
    let steps: Vec<String> = report
        .steps
        .iter()
        .map(|step| {
            format!(
                "{{\"name\":{},\"ms\":{:.3},\"note\":{}}}",
                json_string(&step.name),
                step.ms,
                json_string(&step.note)
            )
        })
        .collect();
    let machine: Vec<String> = report
        .machine
        .iter()
        .map(|(key, value)| format!("{}:{}", json_string(key), json_string(value)))
        .collect();
    format!(
        "{{\"kind\":\"yanmo-acceptance\",\"engine\":\"{}\",\"chapters\":{},\"chars\":{},\"db_bytes\":{},\"machine\":{{{}}},\"steps\":[{}]}}",
        yanmo_core::engine_version(),
        report.chapters,
        report.chars,
        report.db_bytes,
        machine.join(","),
        steps.join(",")
    )
}

fn to_markdown(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 研墨验收报告（{}）\n\n", yanmo_core::engine_version()));
    out.push_str("> 这份报告是 `--self-test` 跑出来的实测数字。**同一份脚本在任何机器上都能跑**，数字随机器变。\n\n");
    out.push_str("## 这台机器\n\n| 项 | 值 |\n| --- | --- |\n");
    for (key, value) in &report.machine {
        out.push_str(&format!("| {key} | {value} |\n"));
    }
    out.push_str(&format!(
        "\n## 这份库\n\n- 章数：{}\n- 总字数：约 {}\n- 库文件：{}\n\n",
        report.chapters,
        report.chars,
        human_bytes(report.db_bytes)
    ));
    out.push_str("## 量出来的数字\n\n| 步骤 | 耗时 | 附注 |\n| --- | ---: | --- |\n");
    for step in &report.steps {
        out.push_str(&format!("| {} | {} | {} |\n", step.name, human_ms(step.ms), step.note));
    }
    // 界面那一步在另一个进程里量（那才是真的冷启动），量完把这行替换掉
    out.push_str(UI_ROW);
    out.push('\n');
    out.push_str("\n---\n\n数字怎么读、怎么自己复现：见仓库里的 `ACCEPTANCE.md` 与 `tools/acceptance/README.md`。\n");
    out
}

/// 体积给人看：小库别显示成 `0 MB`。
fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

/// 耗时给人看：不到 1 毫秒就说微秒，免得一堆 `0.04 ms` 看不出差别。
fn human_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.0} 微秒", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{ms:.1} 毫秒")
    } else {
        format!("{:.2} 秒", ms / 1000.0)
    }
}

/// JSON 字符串转义（只处理必须处理的几个字符）。
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── 第二步：界面冷启动计时（另一个进程，才是真的冷启动） ──────────────────────

/// 界面那一步的计划（进程级只存一次）。
static UI_PLAN: std::sync::OnceLock<Plan> = std::sync::OnceLock::new();
/// 决定开窗的那一刻（"冷启动"从这儿算起，不含前面造数据的时间）。
static UI_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// 报告里占位的那一行：第二步量完会把它换掉。
const UI_ROW: &str = "| 界面冷启动到可输入 | 待测 | 由第二步填入 |";

pub fn install_ui(plan: Plan) {
    let _ = UI_PLAN.set(plan);
}

/// 界面就绪的等待上限：到点还没就绪就如实写进报告并退出。
///
/// **绝不能挂在那儿等**——验收脚本是给作者双击的，一个永远不结束的窗口
/// 比一个"数字没量到"的失败难查得多。
const UI_DEADLINE: std::time::Duration = std::time::Duration::from_secs(90);

/// 页面到底加载过没有：区分"页面根本没加载"和"页面加载了但前端没走到就绪"。
static PAGE_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 页面加载完成的那一刻（由 `on_page_load` 记下）。
pub fn note_page_loaded() {
    PAGE_LOADED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// 界面这一步是否已经有结论（就绪或超时）——两件事只允许发生一件。
static UI_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 看门狗要等的时长。
pub fn ui_deadline() -> std::time::Duration {
    UI_DEADLINE
}

pub fn ui_plan() -> Option<&'static Plan> {
    UI_PLAN.get()
}

/// 开窗之前调用：这一刻才算"冷启动"的起点。
pub fn note_ui_start() {
    let _ = UI_START.set(Instant::now());
}

/// 界面那一步的**命令级日志**：只写一行"第几毫秒调了什么"。
///
/// 为什么要有它：界面没就绪时，"页面没加载"和"前端走到某一步就报错了"是两回事，
/// 光看结果分不出来。这份日志让排查一眼就定位（不写日志文件就永远只能猜）。
pub fn note_command(name: &str) {
    let Some(plan) = ui_plan() else { return };
    let ms = UI_START
        .get()
        .map(|started| started.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let path = PathBuf::from(format!("{}-ui.log", plan.report.display()));
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(file, "{ms:>9.1}ms  {name}");
    }
}

/// 界面在期限前没就绪：写清原因，报告照样出来，然后退出（非零退出码＝这一步没成）。
pub fn ui_deadline_passed(app: &tauri::AppHandle) {
    let Some(plan) = ui_plan() else { return };
    if UI_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return; // 界面已经就绪过了，看门狗下班
    }
    let note = if PAGE_LOADED.load(std::sync::atomic::Ordering::SeqCst) {
        format!(
            "界面 {:.0} 秒内没能就绪：页面加载过了，但前端没走到可用（多半是前端报错，看 -ui.log）",
            UI_DEADLINE.as_secs_f64()
        )
    } else {
        format!(
            "界面 {:.0} 秒内没能就绪：**页面根本没加载**——这一份多半是开发构建（要本地前端服务），换成安装版的 研墨.exe 再跑",
            UI_DEADLINE.as_secs_f64()
        )
    };
    if let Err(error) = merge_ui_note(&plan.report, None, &note) {
        // i18n-allow-next-line: 开发者排查看的告警，不是界面文案
        eprintln!("验收报告没写成：{error}");
    }
    app.exit(3);
}

/// 界面就绪（前端把退出闸门武装上的那一刻）——量完写进报告，然后让壳退出。
///
/// 放在后台线程里等一小会儿再退：先把这条命令的回应给前端，界面不至于吊在半空。
pub fn ui_ready(app: &tauri::AppHandle) {
    let Some(plan) = ui_plan() else { return };
    if UI_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return; // 已经收过尾了（看门狗先到）
    }
    let Some(started) = UI_START.get() else { return };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let prefix = plan.report.clone();
    if let Err(error) = merge_ui_note(&prefix, Some(ms), "从开窗到界面可用（含读回这一章）") {
        // i18n-allow-next-line: 开发者排查看的告警，不是界面文案
        eprintln!("验收报告没写成：{error}");
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        handle.exit(0);
    });
}

/// 把界面那一步并进第一步写好的报告（同一个前缀下最新的一份）。
///
/// `ms` 为 `None` 表示这一步**没量到**（比如界面超时）：只把原因写进附注，不编一个数字。
fn merge_ui_note(prefix: &Path, ms: Option<f64>, note: &str) -> std::io::Result<()> {
    let dir = prefix.parent().filter(|path| !path.as_os_str().is_empty()).map(Path::to_path_buf);
    let stem = prefix
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "验收报告".to_string());
    let search_dir = dir.clone().unwrap_or_else(|| PathBuf::from("."));
    let mut found: Vec<PathBuf> = std::fs::read_dir(&search_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with(&stem))
                .unwrap_or(false)
        })
        .collect();
    found.sort();
    let md_path = found.iter().rev().find(|path| path.extension().is_some_and(|ext| ext == "md"));
    let json_path = found.iter().rev().find(|path| path.extension().is_some_and(|ext| ext == "json"));
    if let Some(path) = md_path {
        let text = std::fs::read_to_string(path)?;
        let row = match ms {
            Some(ms) => format!("| 界面冷启动到可输入 | {} | {note} |", human_ms(ms)),
            None => format!("| 界面冷启动到可输入 | 没量到 | {note} |"),
        };
        std::fs::write(path, text.replace(UI_ROW, &row))?;
    }
    if let Some(path) = json_path {
        let text = std::fs::read_to_string(path)?;
        if let Some(index) = text.rfind(']') {
            let step = format!(
                ",{{\"name\":{},\"ms\":{},\"note\":{}}}",
                json_string("界面冷启动到可输入"),
                ms.map(|value| format!("{value:.3}")).unwrap_or_else(|| "null".to_string()),
                json_string(note)
            );
            let merged = format!("{}{}{}", &text[..index], step, &text[index..]);
            std::fs::write(path, merged)?;
        }
    }
    Ok(())
}
