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

/// 这一份程序支不支持验收模式（`--version` 会把它报出去，脚本据此挡老版本）。
pub const SUPPORTED: u32 = 1;

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
    /// `--check` 的结果写哪个文件。
    ///
    /// **不能靠 stdout**：研墨是 GUI 子系统程序（`windows_subsystem = "windows"`），
    /// 在 cmd 里拿不到可用的标准输出——`println!` 写了也看不见（演练当场踩到）。
    /// 所以体检结果一律落文件，脚本用 `type` 打出来给人看，文件本身也留作证据。
    pub out: Option<PathBuf>,
    pub chapters: usize,
    pub chars: usize,
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
    /// 沙箱自检没过时写明原因——这一轮**什么都没做**（那些目录一个字节都没删）。
    pub refused: Option<String>,
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

/// 沙箱记号：验收模式在自己造的数据目录里留一个文件。
/// **清空一个目录之前必须先看到它**——没有这个记号、又不在系统临时目录里的目录，不该由我们删。
const SCRATCH_MARKER: &str = ".yanmo-acceptance-scratch";

/// 清空数据目录之前，必须先证明「这是我们自己的沙箱」。
///
/// 为什么这道门必须有：`run_bench` 第一件事就是把数据目录清空重来（上一次的数字不该混进这一次），
/// 而数据目录由 `--dir` 参数给。参数打错一个字、或者从批处理里透传进来一个路径，
/// 就会把作者的真稿库连同里面的备份包一起 `remove_dir_all` 掉——**不可恢复**。
/// 所以放行的只有两种目录：
///
/// ① 系统临时目录之下的**空目录**（自定义落点第一次跑时就是这种：还没东西可删）；
/// ② 带沙箱记号文件的（验收模式自己造过、并留了记号的目录）。
///
/// 另外，**只要里面有稿库又没记号，一律拒绝**——哪怕它落在临时目录里。
/// 唯一的例外是那个默认沙箱路径（见 [`is_our_scratch`]）。
/// 拒绝时一个字节都不动，并且把原因写成报告里的第一步。
fn wipe_guard(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(()); // 还没有这个目录：下面会建，没有东西可删
    }
    let real = dir
        .canonicalize()
        .map_err(|error| format!("路径读不出来（{}）：{error}", dir.display()))?;
    if real.parent().is_none() {
        return Err(format!("拒绝清理 {}：那是盘根目录。", real.display()));
    }
    let temp = std::env::temp_dir();
    let temp = temp.canonicalize().unwrap_or(temp);
    let ours = is_our_scratch(&real, &temp, real.join(SCRATCH_MARKER).is_file());
    if !ours && !real.starts_with(&temp) {
        return Err(format!(
            "拒绝清理 {}：它既不在系统临时目录下，也没有验收沙箱的记号（{SCRATCH_MARKER}）。\
             验收模式只清自己造的目录——换一个空目录，或者直接用默认的临时目录。",
            real.display()
        ));
    }
    if !ours && real.join(yanmo_core::paths::DB_FILE).is_file() {
        return Err(format!(
            "拒绝清理 {}：里面有一份稿库（{}），却不像验收沙箱。\
             验收模式绝不碰真稿库——请换一个空目录。",
            real.display(),
            yanmo_core::paths::DB_FILE
        ));
    }
    Ok(())
}

/// 这个目录算不算"验收自己的沙箱"（纯函数，便于单测）。
///
/// 两种算：① 带我们写的记号文件；② **就是那个默认沙箱路径**（`<临时目录>/yanmo-acceptance`）。
///
/// 为什么单列第 ② 条：0.50.0 及更早建的默认沙箱里没有记号文件，而"有稿库又无记号"那条规则
/// 会把老用户的验收工具直接卡死——升级一次版本不该让人连自检都跑不了。默认路径是**我们定义的**
/// 临时落点，按定义就是我们的（真稿库长在 `%TEMP%\yanmo-acceptance` 的概率可以忽略）。
///
/// 这条是**冒烟测试当场抓出来的**：新包跑默认路径返回了拒绝（退出码 4）。
fn is_our_scratch(real: &Path, temp: &Path, marked: bool) -> bool {
    marked || real == temp.join("yanmo-acceptance")
}

/// 造数据 + 量核心操作（**不开窗口**）。返回报告。
pub fn run_bench(plan: &Plan) -> Report {
    let mut report = Report { machine: machine(), ..Report::default() };
    let started = Instant::now();
    // 每次都从干净目录开始：上一次的结果不该混进这一次的数字。
    // 但**先得证明这目录是我们的**——`--dir` 一路过来没有任何信任可言（见 `wipe_guard`）。
    if let Err(reason) = wipe_guard(&plan.dir) {
        report.refused = Some(reason.clone());
        report.steps.push(Step { name: "造数据".to_string(), ms: 0.0, note: reason });
        return report;
    }
    let _ = std::fs::remove_dir_all(&plan.dir);
    if let Err(error) = std::fs::create_dir_all(&plan.dir) {
        report.steps.push(Step {
            name: "造数据".to_string(),
            ms: 0.0,
            note: format!("建目录失败：{error}"),
        });
        return report;
    }
    // 留下记号：下一次跑同一个目录时，它就是"这是我们造的"的凭据
    let _ = std::fs::write(plan.dir.join(SCRATCH_MARKER), b"yanmo acceptance scratch\n");

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
    // 备份落点放在沙箱**里面**：它一会儿要被删掉（这一轮量完就清），
    // 放在 `--dir` 的兄弟位置等于又造出一处"没证明过是我们的"目录。
    let backup_to = plan.dir.join("backup-out");
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

/// 只读体检一份库（给"不丢稿"演练核对用）：把 JSON **写到文件**（`--out`），返回退出码。
///
/// 为什么写文件而不是打印：研墨是 GUI 子系统程序，在 cmd 控制台里标准输出是无效的，
/// `println!` 写了看不见（演练当场踩到）。文件不会骗人，脚本再用 `type` 打出来。
/// 体检本身不改稿库；`Store::open` 可能写 `-wal`，那是 SQLite 的正常行为。
pub fn run_check(plan: &Plan) -> i32 {
    let db_path = plan.dir.join(yanmo_core::paths::DB_FILE);
    let json = match Store::open(&db_path) {
        Ok(store) => {
            let integrity = db::quick_check(store.conn()).unwrap_or_else(|error| error.to_string());
            let works = store.list_works().map(|list| list.len()).unwrap_or(0);
            let mut chapters = 0i64;
            let mut words = 0i64;
            for entry in store.shelf().unwrap_or_default() {
                chapters += entry.chapters;
                words += entry.word_count;
            }
            format!(
                "{{\"ok\":true,\"integrity\":\"{integrity}\",\"works\":{works},\"chapters\":{chapters},\"words\":{words}}}"
            )
        }
        Err(error) => format!("{{\"ok\":false,\"error\":{}}}", json_string(&error.to_string())),
    };
    // 落到文件（有 --out 就听它的；没给就放在库旁边）
    let target = plan
        .out
        .clone()
        .unwrap_or_else(|| plan.dir.join("yanmo-check.json"));
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&target, format!("{json}\n")) {
        // i18n-allow-next-line: 命令行的机器可读输出（给脚本看），不是界面文案
        eprintln!("体检结果没写成：{error}");
        return 2;
    }
    // 顺带写一份 stdout（从管道/文件重定向读的时候能看到；控制台里看不到是正常的）
    println!("{json}");
    if json.contains("\"ok\":false") {
        return 2;
    }
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
    let refused = report
        .refused
        .as_ref()
        .map(|reason| json_string(reason))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"kind\":\"yanmo-acceptance\",\"refused\":{},\"engine\":\"{}\",\"chapters\":{},\"chars\":{},\"db_bytes\":{},\"machine\":{{{}}},\"steps\":[{}]}}",
        refused,
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
    if let Some(reason) = &report.refused {
        out.push_str(&format!(
            "> **这一轮没有跑。** {reason}\n>\n> 沙箱自检没过，验收模式**什么都没做**：那个目录、以及里面的东西，一个字节都没动。\n\n"
        ));
    }
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

/// 记下界面自测的计划（第二步量完会往报告里填结果）。
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

/// 还没量完时报告里用的占位计划。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个"像真稿库"的目录：里面有库文件，还有一个作者自己放的备份包。
    /// 用 `TempDir` 自动清理——测试里不去碰 `remove_dir_all`（那正是本文件要盯住的动作）。
    fn library_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(yanmo_core::paths::DB_FILE), b"REAL LIBRARY - must survive")
            .unwrap();
        std::fs::write(dir.path().join("我的备份包.zip"), b"author's own backup").unwrap();
        dir
    }

    /// **这条就是那个不可恢复删库漏洞的守卫**：一个带稿库、却没有沙箱记号的目录，
    /// 哪怕它落在系统临时目录里，也一律拒绝——而且拒绝之后库必须原封不动。
    #[test]
    fn refuses_a_directory_that_holds_a_library() {
        let dir = library_dir();
        let refused = wipe_guard(dir.path()).unwrap_err();
        assert!(refused.contains("稿库"), "拒绝理由要让人看懂是稿库：{refused}");
        assert_eq!(
            std::fs::read(dir.path().join(yanmo_core::paths::DB_FILE)).unwrap(),
            b"REAL LIBRARY - must survive",
            "拒绝之后库文件必须一个字节都没动"
        );
    }

    /// 不在系统临时目录下、又没有记号：同样拒绝（`--dir` 指向别处时的那一路）。
    #[test]
    fn refuses_an_unmarked_directory_outside_temp() {
        let app_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let refused = wipe_guard(&app_dir).unwrap_err();
        assert!(
            refused.contains("临时目录"),
            "拒绝理由要说清「既不在临时目录、也没有记号」：{refused}"
        );
    }

    /// 第一次跑：目录还不存在 → 放行（没有东西可删，下面会建）。
    #[test]
    fn allows_a_directory_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("还没有这个目录");
        assert!(wipe_guard(&missing).is_ok(), "不存在的目录没有东西可删，该放行");
    }

    /// 自己造的沙箱（有记号）：里面有上一次验收留下的库也该能清掉。
    #[test]
    fn allows_a_marked_sandbox_even_with_a_library_inside() {
        let dir = library_dir();
        std::fs::write(dir.path().join(SCRATCH_MARKER), b"yanmo acceptance scratch\n").unwrap();
        assert!(wipe_guard(dir.path()).is_ok(), "上一次验收自己造的库，这次该能清");
    }

    /// 端到端：整轮跑在"看起来像真稿库"的目录上时，必须**拒绝执行**且什么都没动。
    #[test]
    fn a_refused_run_does_not_touch_the_directory() {
        let dir = library_dir();
        let plan = Plan { dir: dir.path().to_path_buf(), ..Plan::default() };

        let report = run_bench(&plan);

        assert!(report.refused.is_some(), "指向真稿库时必须拒绝执行");
        assert_eq!(
            std::fs::read(dir.path().join(yanmo_core::paths::DB_FILE)).unwrap(),
            b"REAL LIBRARY - must survive"
        );
        assert_eq!(
            std::fs::read(dir.path().join("我的备份包.zip")).unwrap(),
            b"author's own backup",
            "作者放在数据目录里的备份包也不许动"
        );
        assert!(
            report.steps.iter().any(|step| step.name == "造数据" && !step.note.is_empty()),
            "拒绝原因要写进报告"
        );
    }

    /// **默认沙箱路径按定义就是我们的**：0.50.0 建的它没有记号文件，升级之后也不该被卡死。
    /// 这条是冒烟测试当场抓出来的——新包跑默认路径返回了拒绝（退出码 4），
    /// 因为那个目录里还躺着上一次验收留下的库。
    #[test]
    fn the_default_scratch_path_is_ours_even_without_a_marker() {
        let temp = PathBuf::from("tmp-root");
        assert!(
            is_our_scratch(&temp.join("yanmo-acceptance"), &temp, false),
            "默认沙箱路径：没有记号也算我们的"
        );
        assert!(is_our_scratch(&temp.join("别处"), &temp, true), "有记号就算我们的");
        assert!(
            !is_our_scratch(&temp.join("我的稿子"), &temp, false),
            "临时目录里别的目录不算我们的（那可能真是稿库落在这儿了）"
        );
        assert!(
            !is_our_scratch(&temp.join("yanmo-acceptance-bak"), &temp, false),
            "名字像但不是那个路径，不算"
        );
    }
}
