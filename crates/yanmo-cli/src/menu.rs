//! 交互菜单：**给作者用的备用导出工具**。
//!
//! 图形界面打不开的时候（显卡、驱动、界面本身出问题），作者不该被迫去研究命令行参数。
//! 所以**不带任何参数**运行本程序就是一个中文菜单：看稿、体检、导出、退出。
//!
//! 这个菜单只做**只读与导出**。整个程序里唯一会写库的那一条（从成稿导入）只在命令行参数里，
//! 而且不给 `--yes` 就只看不写——分档见 `commands.rs` 的说明。
//!
//! # 两条硬纪律
//!
//! 1. **绝不静默创建空库**：数据目录里没有 `yanmo.db` 就直说"没找到"，绝不建一个空的
//!    让人以为稿子没了——那是救援工具最不可原谅的失败。
//! 2. 终端界面的文案写在本模块；桌面界面的文案在前端字典——**两个界面，各自一份**，互不冒充。
//!
//! 菜单的输入输出都是注入的（`BufRead` / `Write`），这样它可以被测试直接驱动。

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use yanmo_core::store::{ExportFormat, Store};
use yanmo_core::{atomic, db, location, paths, version};

use crate::commands;

/// 救援导出的目录名（放在作者的"文档"里）。**语言无关**——它会留在磁盘上跟着稿子走。
const RESCUE_FOLDER: &str = "RescueExport";
/// "读取稿子"里一次最多显示多少行（几十万字刷屏没有意义，完整内容请导出）。
const PREVIEW_LINES: usize = 60;

/// 交互入口：解析数据目录 → 打开库 → 进菜单。返回进程退出码。
pub fn run(argv: &[String]) -> i32 {
    let mut out = std::io::stdout();
    let mut input = std::io::stdin().lock();
    match run_with(argv, &mut out, &mut input) {
        Ok(()) => 0,
        Err(message) => {
            // 走到这里说明连菜单都进不去：把原因讲清楚（作者可能正着急）
            let _ = writeln!(out, "\n{message}");
            let _ = writeln!(out, "\n按回车键关闭…");
            let mut line = String::new();
            let _ = input.read_line(&mut line);
            1
        }
    }
}

/// 菜单主体（输入输出注入，便于测试）。
pub fn run_with(
    argv: &[String],
    out: &mut impl Write,
    input: &mut impl BufRead,
) -> Result<(), String> {
    run_with_root(argv, None, out, input)
}

/// 同上，但导出落点由调用方给定（测试用；也方便将来做"导出到指定目录"）。
pub fn run_with_root(
    argv: &[String],
    export_root: Option<&Path>,
    out: &mut impl Write,
    input: &mut impl BufRead,
) -> Result<(), String> {
    let requested = requested_data_dir(argv);
    let data_dir = pick_data_dir(requested, out, input)?;
    let store = open_rescue(&data_dir)?;
    let root = match export_root {
        Some(path) => path.to_path_buf(),
        None => requested_export_root(argv).unwrap_or_else(|| export_root_for(&data_dir)),
    };
    banner(out, &data_dir, &root, &store)?;
    menu_loop(&store, &root, out, input)
}

/// 参数里有没有 `--data <目录>`（菜单模式下也认它，方便知道自己在哪的人直接用）。
fn requested_data_dir(argv: &[String]) -> Option<PathBuf> {
    value_of(argv, "--data")
}

/// `--export-to <目录>`：想把稿子导到别处（比如 U 盘）时用它。
fn requested_export_root(argv: &[String]) -> Option<PathBuf> {
    value_of(argv, "--export-to")
}

fn value_of(argv: &[String], flag: &str) -> Option<PathBuf> {
    let mut index = 0;
    while index < argv.len() {
        if argv[index] == flag {
            return argv.get(index + 1).map(PathBuf::from);
        }
        index += 1;
    }
    None
}

/// 定下要用哪个数据目录：给了就用给的；没给就按系统约定找；找不到就问作者。
fn pick_data_dir(
    requested: Option<PathBuf>,
    out: &mut impl Write,
    input: &mut impl BufRead,
) -> Result<PathBuf, String> {
    if let Some(dir) = requested {
        if has_database(&dir) {
            return Ok(dir);
        }
        return Err(format!(
            "这个目录里没有找到稿子库：{}\n（请确认路径，或直接运行本程序让我自动找）",
            dir.display()
        ));
    }
    // 与图形界面**同一份判定**（`core::location`）：位置记录优先 → 老位置认领 → 首启推荐。
    // 两个壳必须说同一句话，否则作者会以为稿子分家了。
    let looked_up = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .map(location::Sources::from_env)
        .and_then(|sources| location::resolve(&sources).map(|found| (sources, found)));
    if let Some((sources, found)) = looked_up {
        if has_database(&found.dir) {
            // 认领/记录来的位置顺手写回记录：下次（以及图形界面）问出的是同一个地方
            if !found.source.is_first_run() {
                if let Some(pointer) = location::pointer_path(&sources) {
                    let _ = location::write_record(&pointer, &found.dir);
                }
            }
            return Ok(found.dir);
        }
        let _ = writeln!(out, "\n在预期位置没找到稿子库：{}", found.dir.display());
    } else {
        let _ = writeln!(out, "\n没能问出系统的数据目录在哪。");
    }
    let _ = writeln!(
        out,
        "你可以在研墨界面顶栏那行小字里看到数据目录（写着「数据 …」），把它粘贴到下面。"
    );
    loop {
        let _ = write!(out, "\n请粘贴数据目录（直接回车退出）：");
        let _ = out.flush();
        let Some(line) = read_line(input) else { return Err("没有拿到输入，先退出了。".to_string()) };
        let text = line.trim().trim_matches('"');
        if text.is_empty() {
            return Err("没有指定数据目录，先退出了。".to_string());
        }
        let dir = PathBuf::from(text);
        if has_database(&dir) {
            return Ok(dir);
        }
        let _ = writeln!(out, "这个目录里没有稿子库（找不到 {}），再试一次？", paths::DB_FILE);
    }
}

fn has_database(dir: &Path) -> bool {
    dir.join(paths::DB_FILE).is_file()
}

/// 打开库——**只可能打开已存在的库**（见 [`pick_data_dir`]），绝不新建。
fn open_rescue(data_dir: &Path) -> Result<Store, String> {
    Store::open(data_dir.join(paths::DB_FILE))
        .map_err(|error| format!("打不开稿子库：{error}\n（数据可能已损坏，可以先把整个数据目录复制一份留底）"))
}

/// 导出落点：优先作者的"文档"（他自己找得到），拿不到就放数据目录旁边。
fn export_root_for(data_dir: &Path) -> PathBuf {
    match paths::documents_dir() {
        Some(documents) => paths::export_root(&documents).join(RESCUE_FOLDER),
        None => data_dir.join("export"),
    }
}

fn banner(
    out: &mut impl Write,
    data_dir: &Path,
    export_root: &Path,
    store: &Store,
) -> Result<(), String> {
    let works = store.shelf().map_err(|error| error.to_string())?;
    let chapters: i64 = works.iter().map(|entry| entry.chapters).sum();
    let words: i64 = works.iter().map(|entry| entry.word_count).sum();
    let _ = writeln!(out, "\n============================================================");
    let _ = writeln!(out, "  研墨 · 备用导出工具");
    let _ = writeln!(out, "  图形界面打不开的时候，用它把稿子取出来");
    let _ = writeln!(out, "============================================================");
    let _ = writeln!(out, "  数据目录：{}", data_dir.display());
    let _ = writeln!(out, "  导出到：  {}", export_root.display());
    let _ = writeln!(out, "  现有稿子：{} 本，共 {} 章，约 {} 字", works.len(), chapters, words);
    // 第一眼就要让作者看到"我的书还在"——只报个数字不够
    for entry in works.iter().take(10) {
        let _ = writeln!(
            out,
            "    · 《{}》{}",
            display_title(&entry.work.title),
            size_label(entry.chapters, entry.word_count)
        );
    }
    if works.len() > 10 {
        let _ = writeln!(out, "    · ……还有 {} 本（用 [1] 逐本看）", works.len() - 10);
    }
    Ok(())
}

/// 一本书的规模怎么写："1 章，8 字" / "8 字"（单篇没有章数）。
fn size_label(chapters: i64, words: i64) -> String {
    if chapters > 0 {
        format!("（{chapters} 章，约 {words} 字）")
    } else {
        format!("（约 {words} 字）")
    }
}

fn menu_loop(
    store: &Store,
    export_root: &Path,
    out: &mut impl Write,
    input: &mut impl BufRead,
) -> Result<(), String> {
    loop {
        let works = store.shelf().map_err(|error| error.to_string())?;
        let _ = writeln!(out, "\n  [1] 读取稿子（看看字还在不在）");
        let _ = writeln!(out, "  [2] 检查数据库健康度");
        let _ = writeln!(out, "  [3] 导出某一本 / 某一篇");
        let _ = writeln!(out, "  [4] 导出全部（{} 本）", works.len());
        let _ = writeln!(out, "  [5] 退出");
        let _ = write!(out, "\n请选择（输入数字后回车）：");
        let _ = out.flush();

        let Some(choice) = read_line(input) else {
            let _ = writeln!(out, "\n（输入结束，退出）");
            return Ok(());
        };
        match choice.trim() {
            "1" => read_works(store, out, input)?,
            "2" => health(store, out)?,
            "3" => export_one(store, export_root, out, input)?,
            "4" => export_all(store, export_root, out)?,
            "5" | "q" | "quit" | "" => {
                let _ = writeln!(out, "\n你的稿子还在「数据目录」那一行显示的位置，一个都没动。");
                let _ = writeln!(out, "再见。");
                return Ok(());
            }
            other => {
                let _ = writeln!(out, "\n「{other}」不是 1-5 里的选项，再试一次。");
            }
        }
    }
}

/// 读稿：列书 → 列章 → 看内容。
fn read_works(store: &Store, out: &mut impl Write, input: &mut impl BufRead) -> Result<(), String> {
    let works = store.shelf().map_err(|error| error.to_string())?;
    if works.is_empty() {
        let _ = writeln!(out, "\n这个库里还没有稿子。");
        return Ok(());
    }
    let labels: Vec<String> = works
        .iter()
        .map(|entry| {
            format!(
                "《{}》（{} 章，约 {} 字）",
                display_title(&entry.work.title),
                entry.chapters,
                entry.word_count
            )
        })
        .collect();
    let Some(index) = choose(out, input, &labels, "要读哪一本？") else { return Ok(()) };
    let work_id = works[index].work.id;

    let nodes = store.list_nodes(work_id).map_err(|error| error.to_string())?;
    let readable: Vec<_> = nodes.iter().filter(|node| node.kind.holds_body()).collect();
    if readable.is_empty() {
        let _ = writeln!(out, "\n这本书里还没有可读的章节。");
        return Ok(());
    }
    let labels: Vec<String> = readable
        .iter()
        .map(|node| {
            format!("{}（{} 字）", display_title(&node.title), node.word_count)
        })
        .collect();
    let Some(index) = choose(out, input, &labels, "读哪一章？") else { return Ok(()) };
    let node = readable[index];

    let body = store.read_body(node.id).map_err(|error| error.to_string())?;
    let lines: Vec<&str> = body.lines().collect();
    let _ = writeln!(out, "\n---------- {} ----------", display_title(&node.title));
    for line in lines.iter().take(PREVIEW_LINES) {
        let _ = writeln!(out, "{line}");
    }
    if lines.len() > PREVIEW_LINES {
        let _ = writeln!(
            out,
            "\n（这一章共 {} 行，上面只显示了前 {} 行；完整内容请用 [3] 导出成文件）",
            lines.len(),
            PREVIEW_LINES
        );
    }
    Ok(())
}

/// 体检：把库文件的完整性说成作者看得懂的一句话。
///
/// **三态**（不只是"是不是 ok"）：查过是好的 / 查过有问题 / **这次没查成**。
/// 最后那一档最要紧：库文件只读时 SQLite 的 FTS5 索引核对需要写权限，它给的原话是
/// "unable to validate … readonly database"——那不是"库坏了"，可作者看到"库文件有问题，
/// 先别做别的操作、赶紧导出留底找开发者"会白白吓一跳（2026-09-14 组合故障演练发现）。
fn health(store: &Store, out: &mut impl Write) -> Result<(), String> {
    let _ = writeln!(out, "\n正在检查数据库……");
    let verdict = db::integrity(store.conn()).map_err(|error| error.to_string())?;
    let schema = db::migrations::user_version(store.conn()).map_err(|error| error.to_string())?;
    let _ = writeln!(out, "  库文件完整性：{}", verdict.raw());
    let _ = writeln!(out, "  结构版本：{schema}");
    let _ = writeln!(out, "  引擎版本：{}", version::engine_version());
    match verdict {
        db::Integrity::Clean => {
            let _ = writeln!(out, "\n结论：看起来没问题。稿子都在，可以放心用 [3] / [4] 导出。");
        }
        db::Integrity::NotChecked(_) => {
            let _ = writeln!(
                out,
                "\n结论：这一次**没能核对**（上面那行就是原因——多半是库文件被设成了只读，\
                 核对索引那一步需要写权限）。\n这不是说库有问题：你的稿子照常读得出来。\
                 \n想核对：先把只读去掉（右键库文件 → 属性 → 取消「只读」），再跑一次 [2]；\
                 \n只想拿稿子：用 [3] / [4] 导出就行。"
            );
        }
        db::Integrity::Problem(_) => {
            let _ = writeln!(
                out,
                "\n结论：库文件有问题（上面那行就是问题所在）。\n建议：先别做别的操作，用 [4] 把还能读出来的稿子导出成文件；\n再把这个数据目录整个复制一份留底，然后找开发者看看。"
            );
        }
    }
    Ok(())
}

/// 导出某一本：选书 → 选格式 → 写文件。
fn export_one(
    store: &Store,
    export_root: &Path,
    out: &mut impl Write,
    input: &mut impl BufRead,
) -> Result<(), String> {
    let works = store.shelf().map_err(|error| error.to_string())?;
    if works.is_empty() {
        let _ = writeln!(out, "\n这个库里还没有稿子。");
        return Ok(());
    }
    let labels: Vec<String> = works
        .iter()
        .map(|entry| format!("《{}》", display_title(&entry.work.title)))
        .collect();
    let Some(index) = choose(out, input, &labels, "导出哪一本？") else { return Ok(()) };
    let entry = &works[index];

    let formats = ["两种都要（推荐）", "只要 txt（纯文本）", "只要 json（带结构）"];
    let Some(kind) = choose(out, input, &formats.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "导出成什么格式？") else {
        return Ok(());
    };
    export_work(store, export_root, entry, kind, out).map(|_| ())
}

/// 导出全部：一本一本导，最后给一句汇总。
fn export_all(store: &Store, export_root: &Path, out: &mut impl Write) -> Result<(), String> {
    let works = store.shelf().map_err(|error| error.to_string())?;
    if works.is_empty() {
        let _ = writeln!(out, "\n这个库里还没有稿子。");
        return Ok(());
    }
    let mut files = 0;
    let mut failed = 0;
    for entry in &works {
        match export_work(store, export_root, entry, 0, out) {
            Ok(count) => files += count,
            Err(_) => failed += 1,
        }
    }
    let _ = writeln!(out, "\n合计导出 {files} 个文件（{} 本）。", works.len() - failed);
    if failed > 0 {
        let _ = writeln!(out, "有 {failed} 本没能导出——上面写了原因。");
    }
    Ok(())
}

/// 真正写文件。`kind`：0 = txt + json，1 = 只要 txt，2 = 只要 json。
fn export_work(
    store: &Store,
    export_root: &Path,
    entry: &yanmo_core::store::ShelfEntry,
    kind: usize,
    out: &mut impl Write,
) -> Result<usize, String> {
    let title = display_title(&entry.work.title);
    let dir = export_root.join(atomic::safe_file_name(&title));
    let wanted: &[ExportFormat] = match kind {
        1 => &[ExportFormat::Text],
        2 => &[ExportFormat::Json],
        _ => &[ExportFormat::Text, ExportFormat::Json],
    };
    let mut written = 0;
    for format in wanted {
        let format = *format;
        written += commands::write_rendered(store, entry.work.id, format, &dir)
            .map_err(|error| format!("导出《{title}》失败：{error}"))?;
    }
    let _ = writeln!(out, "  ✅ 《{title}》→ {}（{written} 个文件）", dir.display());
    Ok(written)
}

/// 让作者从一份清单里选一个（回车 = 不选，回到菜单）。
fn choose(
    out: &mut impl Write,
    input: &mut impl BufRead,
    labels: &[String],
    title: &str,
) -> Option<usize> {
    let _ = writeln!(out, "\n{title}");
    for (index, label) in labels.iter().enumerate() {
        let _ = writeln!(out, "  [{}] {label}", index + 1);
    }
    let _ = write!(out, "请输入编号（直接回车返回）：");
    let _ = out.flush();
    let line = read_line(input)?;
    let text = line.trim();
    if text.is_empty() {
        return None;
    }
    match text.parse::<usize>() {
        Ok(number) if number >= 1 && number <= labels.len() => Some(number - 1),
        _ => {
            let _ = writeln!(out, "没看懂「{text}」，回到菜单。");
            None
        }
    }
}

/// 读一行；读到结尾（比如输入被关掉）返回 `None`。
fn read_line(input: &mut impl BufRead) -> Option<String> {
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line),
        Err(_) => None,
    }
}

/// 名字可能为空（没起名的书 / 卷）：显示的永远是"有话说"的名字。
fn display_title(title: &str) -> String {
    if title.trim().is_empty() {
        "（未命名）".to_string()
    } else {
        title.to_string()
    }
}
