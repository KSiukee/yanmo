//! 命令的执行：**发布构建只留「救稿子」的那几条，开发/演练构建保留全部**。
//!
//! # 两档命令面（编译期分档，不是运行期开关）
//!
//! - **只读档（任何构建都有）**：`verify` / `works` / `nodes` / `read` / `search` / `export`——
//!   全部是**只读或只写文件**的，**一条都不能改稿库**。界面起不来、库有问题的时候，
//!   靠这几条判断"字还在不在"并把它取出来。
//! - **写库档（任何构建都有，只此一条）**：`import`——从成稿 JSON 把整本书**新建**回来。
//!   它不碰既有数据，且**不给 `--yes` 就只看不写**（实现在 `rescue_import`，私有模块不进文档）。
//! - **开发档（仅开发构建）**：`begin` / `report` / `note-open` / `write` / `fingerprint` /
//!   `end` / `abandon` / `new-work` / `new-node` / `hold`——给自动化与场景复现用的。
//!   发布构建里**整个模块都不编译**（见 `dev.rs`），不是"藏起来"，是根本不存在。
//!
//! 判据用 `debug_assertions`：开发构建全功能、发布构建自动剃除，**不靠谁记得加参数**。
//!
//! 业务一律在 `yanmo-core`：这里只做「参数转换 + 调核心 + 组装 JSON」，
//! 不拼人类句子（成功给数据、失败给「码 + 参数」）。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use yanmo_core::db;
use yanmo_core::store::{ExportFormat, Store};
use yanmo_core::version;

#[cfg(debug_assertions)]
use crate::dev;
use crate::args::{Args, Usage};
use crate::CliError;

/// 数据目录里的库文件名（与桌面壳同一份约定，见 `app/src/storage.rs`）。
const DB_FILE: &str = "yanmo.db";

/// **救援档**允许哪些选项——写错一个字母会当场报错，不会被静默忽略。
fn rescue_options(command: &str) -> Option<&'static [&'static str]> {
    match command {
        "verify" => Some(&[]),
        "works" => Some(&[]),
        "nodes" => Some(&["work"]),
        "read" => Some(&["node"]),
        "search" => Some(&["query", "work", "limit"]),
        "export" => Some(&["work", "format", "out"]),
        "import" => Some(&["from", "work", "language", "yes"]),
        _ => None,
    }
}

/// 命令是否存在、允许哪些选项。开发档只在开发构建里登记。
fn allowed_options(command: &str) -> Result<Vec<&'static str>, Usage> {
    let known = rescue_options(command).or_else(|| {
        #[cfg(debug_assertions)]
        {
            dev::options(command)
        }
        #[cfg(not(debug_assertions))]
        {
            None
        }
    });
    let Some(known) = known else {
        return Err(Usage(format!("不认识的命令：{command}")));
    };
    #[allow(unused_mut)]
    let mut known = known.to_vec();
    // 调试注入参数只在带测试特性编译时才是"已知选项"；发布构建里连登记都没有
    #[cfg(feature = "testing")]
    known.push("max-pages");
    Ok(known)
}

/// 打开数据目录里的库。
///
/// **不做"目录不存在就建、库不存在就开一个新的"**：`--data` 写错一个字符时，
/// `verify` / `works` / `read` 这些命令会当场造出一个空库、再一本正经地回"没有作品"——
/// 作者看到的正是"稿子没了"（2026-09-15 代码质量评审：轻微 16/26；壳侧同类问题已在严重 3 修掉，
/// CLI 这条一直没堵）。库由**研墨本体**创建（首启 / 建书），命令行只认已经存在的库。
///
/// 错误只给码与参数（CLI 不拼人类句子，见模块头）：`store.missing` + `path`。
pub(crate) fn open_store(dir: &Path) -> Result<Store, CliError> {
    let db = dir.join(DB_FILE);
    if !db.is_file() {
        return Err(yanmo_core::Error::invalid_with(
            yanmo_core::error_codes::codes::STORE_MISSING,
            [("path", dir.display().to_string())],
        )
        .into());
    }
    Ok(Store::open(db)?)
}

/// 允许**建库**的那一条路：只有"本来就是在造新东西"的命令才配走它。
///
/// 见 [`may_create_library`]：出货的 CLI 里一条都没有（`new-work` 是开发档命令），
/// 所以发布出去的二进制**永远不会**替你造出一个空库。
fn open_store_creating(dir: &Path) -> Result<Store, CliError> {
    std::fs::create_dir_all(dir)?;
    Ok(Store::open(dir.join(DB_FILE))?)
}

/// 这条命令允许把库开出来吗（＝库里没有就建一个新的）。
///
/// 只有 `new-work` 算——它就是"开一本新书"，测试与脚本也拿它给空目录播种。
/// 其余一律只认已经存在的库（2026-09-15 代码质量评审：轻微 16/26）。
fn may_create_library(command: &str) -> bool {
    command == "new-work"
}

/// 调试用的注入参数：给库设一个**页数上限**，让后续写入真的走到"写不下"这条路。
///
/// 只在带测试特性编译时存在；发布构建里这个函数体是空的（参数本身也没登记）。
#[cfg(feature = "testing")]
pub(crate) fn apply_debug_limits(store: &Store, args: &Args) -> Result<(), CliError> {
    if let Some(text) = args.options.get("max-pages").filter(|value| !value.is_empty()) {
        let pages: i64 = text.parse().map_err(|_| Usage::from("--max-pages 需要是一个整数"))?;
        store
            .conn()
            .pragma_update(None, "max_page_count", pages)
            .map_err(yanmo_core::Error::Db)?;
    }
    Ok(())
}

#[cfg(not(feature = "testing"))]
pub(crate) fn apply_debug_limits(_store: &Store, _args: &Args) -> Result<(), CliError> {
    Ok(())
}

/// 执行一条命令，产出要打印的 JSON。
pub fn execute(args: &Args) -> Result<Value, CliError> {
    // **先把命令与选项认全，再去碰磁盘**：`allowed_options` 对不认识的命令会报用法错误。
    // 这一步原先只在"带了选项"时才触发，于是 `dance`（不带选项）会先去看库，
    // 把"命令写错了"报成"这里没有稿库"——既有测试当场抓出来的。
    let allowed = allowed_options(&args.command)?;
    for name in args.options.keys() {
        if !allowed.contains(&name.as_str()) {
            return Err(Usage(format!("命令 {} 不认识选项 --{name}", args.command)).into());
        }
    }

    let mut store = if may_create_library(&args.command) {
        open_store_creating(&args.data)?
    } else {
        open_store(&args.data)?
    };
    apply_debug_limits(&store, args)?;

    if let Some(value) = rescue(args, &mut store)? {
        return Ok(value);
    }
    #[cfg(debug_assertions)]
    if let Some(value) = dev::execute(args, &mut store)? {
        return Ok(value);
    }
    // 走到这里只有一种可能：发布构建里来了条开发档命令（它在这个构建里不存在）
    Err(Usage(format!("不认识的命令：{}", args.command)).into())
}

/// 可选的整数选项：没给就是 `None`；给了但不是整数**当场报用法错误**（不静默当没给）。
fn parse_optional_i64(value: Option<&str>, name: &str) -> Result<Option<i64>, CliError> {
    match value {
        None => Ok(None),
        Some(text) => {
            text.parse::<i64>().map(Some).map_err(|_| Usage(format!("{name} 需要是一个整数")).into())
        }
    }
}

/// 救援档：**看稿 / 导出 / 从成稿导入**——不碰作者的稿子，但都会打开库。
///
/// 措辞注意（评审：中等 4）：`Store::open` 会跑结构迁移、登记本机、给老库回填字数标记，
/// 所以"一条都不改稿库"是句错话（改的不是稿子，是库文件本身）；帮助文本已按事实改过。
fn rescue(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    let value = match args.command.as_str() {
        "verify" => {
            // 打开这一步就已经跑过环境校验与迁移：库坏了 / 版本过新都会在这里明确报错
            let verdict = db::integrity(store.conn())?;
            let schema = db::migrations::user_version(store.conn())?;
            json!({
                "ok": true,
                "command": "verify",
                // 原话（日志与报告留档用）与**结论**分开给：
                // clean / problem / not_checked（只读库上 FTS5 那一步核对不了）
                "integrity": verdict.raw(),
                "integrity_state": verdict.as_str(),
                "schema_version": schema,
                "engine_version": version::engine_version(),
            })
        }
        "works" => {
            let works: Vec<Value> = store
                .shelf()?
                .into_iter()
                .map(|entry| {
                    json!({
                        "id": entry.work.id,
                        "kind": entry.work.kind.as_str(),
                        "title": entry.work.title,
                        "chapters": entry.chapters,
                        "word_count": entry.word_count,
                    })
                })
                .collect();
            json!({ "ok": true, "command": "works", "works": works })
        }
        "nodes" => {
            let work = args.required_i64("work")?;
            let nodes: Vec<Value> = store
                .list_nodes(work)?
                .into_iter()
                .map(|node| {
                    json!({
                        "id": node.id,
                        "parent_id": node.parent_id,
                        "kind": node.kind.as_str(),
                        "title": node.title,
                        "word_count": node.word_count,
                        "has_body": node.has_body,
                        "has_children": node.has_children,
                    })
                })
                .collect();
            json!({ "ok": true, "command": "nodes", "nodes": nodes })
        }
        "read" => {
            let node = args.required_i64("node")?;
            let body = store.read_body(node)?;
            json!({ "ok": true, "command": "read", "node_id": node, "body": body })
        }
        "search" => {
            let query = args.required("query")?;
            let work = parse_optional_i64(args.optional("work"), "--work")?;
            // 不给 --limit 就用核心的默认上限（给 0 是同一个意思，见 store::search）
            let limit = parse_optional_i64(args.optional("limit"), "--limit")?.unwrap_or(0);
            let limited = u32::try_from(limit).map_err(|_| Usage::from("--limit 不能是负数"))? as usize;
            let hits: Vec<Value> = store
                .search(query, work, limited)?
                .into_iter()
                .map(|hit| {
                    json!({
                        "node_id": hit.node_id,
                        "work_id": hit.work_id,
                        "title": hit.title,
                        "snippet": hit.snippet,
                        "matched_title": hit.matched_title,
                    })
                })
                .collect();
            json!({
                "ok": true,
                "command": "search",
                "query": query,
                "work_id": work,
                "limit": limit,
                "count": hits.len(),
                "hits": hits,
            })
        }
        "export" => {
            let work = args.required_i64("work")?;
            let format = ExportFormat::parse(args.required("format")?)?;
            let out = PathBuf::from(args.required("out")?);
            let written = write_rendered(store, work, format, &out)?;
            json!({ "ok": true, "command": "export", "files": written, "dir": out.display().to_string() })
        }
        // 唯一会写库的一条：它自己讲清楚了"默认只看不写"，所以单独一处（见 rescue_import）
        "import" => return crate::rescue_import::run(args, store).map(Some),
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// 把渲染好的文件写到导出目录：**内容一样就不重写**（与桌面壳同一条纪律）。
pub(crate) fn write_rendered(
    store: &Store,
    work_id: i64,
    format: ExportFormat,
    out: &Path,
) -> Result<usize, CliError> {
    let files = store.render_work(work_id, format)?;
    let mut written = 0;
    for file in &files {
        let path = out.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 比字节：产物可能是二进制（docx），按字符串比会永远"不相等"而反复重写
        let unchanged = std::fs::read(&path).map(|old| old == file.content).unwrap_or(false);
        if unchanged {
            continue;
        }
        yanmo_core::atomic::write_atomic(&path, &file.content)?;
        written += 1;
    }
    Ok(written)
}
