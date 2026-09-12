//! 命令的执行：**发布构建只留「救稿子」的那几条，开发/演练构建保留全部**。
//!
//! # 两档命令面（编译期分档，不是运行期开关）
//!
//! - **救援档（任何构建都有）**：`verify` / `works` / `nodes` / `read` / `export`——
//!   全部是**只读或只写文件**的，**一条都不能改稿库**。界面起不来、库有问题的时候，
//!   靠这几条判断"字还在不在"并把它取出来。
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
        "export" => Some(&["work", "format", "out"]),
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

/// 打开数据目录里的库（目录不存在就建），完成环境校验与结构迁移。
pub(crate) fn open_store(dir: &Path) -> Result<Store, CliError> {
    std::fs::create_dir_all(dir)?;
    Ok(Store::open(dir.join(DB_FILE))?)
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
    for name in args.options.keys() {
        if !allowed_options(&args.command)?.contains(&name.as_str()) {
            return Err(Usage(format!("命令 {} 不认识选项 --{name}", args.command)).into());
        }
    }

    let mut store = open_store(&args.data)?;
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

/// 救援档：**只读或只写文件**，一条都不改稿库。
fn rescue(args: &Args, store: &mut Store) -> Result<Option<Value>, CliError> {
    let value = match args.command.as_str() {
        "verify" => {
            // 打开这一步就已经跑过环境校验与迁移：库坏了 / 版本过新都会在这里明确报错
            let integrity = db::quick_check(store.conn())?;
            let schema = db::migrations::user_version(store.conn())?;
            json!({
                "ok": true,
                "command": "verify",
                "integrity": integrity,
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
        "export" => {
            let work = args.required_i64("work")?;
            let format = ExportFormat::parse(args.required("format")?)?;
            let out = PathBuf::from(args.required("out")?);
            let written = write_rendered(store, work, format, &out)?;
            json!({ "ok": true, "command": "export", "files": written, "dir": out.display().to_string() })
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// 把渲染好的文件写到导出目录：**内容一样就不重写**（与桌面壳同一条纪律）。
fn write_rendered(
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
        let unchanged = std::fs::read_to_string(&path).map(|old| old == file.content).unwrap_or(false);
        if unchanged {
            continue;
        }
        yanmo_core::atomic::write_atomic(&path, file.content.as_bytes())?;
        written += 1;
    }
    Ok(written)
}
