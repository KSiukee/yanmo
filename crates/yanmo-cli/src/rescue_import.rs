//! 从成稿导入（命令行）：**默认干跑，加 `--yes` 才动库**。
//!
//! # 为什么它敢待在发行版里
//!
//! 库打不开的时候，作者手里能用的只有一个命令行 + 一份备份包。这条路只读成稿 JSON
//! （不碰快照、不碰 SQLite），把整本书**新建**成一本书——既有数据一个字都不动。
//! 所以规矩定得很死：
//!
//! - 不给 `--yes` 就**只看不写**：找成稿、验格式、与备份清单对账全都照跑，
//!   让作者先看清楚"这一份里有几章、多少字、跟清单对不对得上"；
//! - 干跑与真写走**同一条路**，只差最后一步——"我看到的数"与"写进去的数"不可能不一样。
//!
//! # 一句话前提
//!
//! 它写的是 `--data` 指的那个库。库文件坏得连打开都不行时，先让研墨开一个新库
//! （把坏掉的库文件挪开、别删），再跑这条命令。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use yanmo_core::model::WorkLanguage;
use yanmo_core::store::{self, Store, WorkDraft, WorkStamp};

use crate::args::{Args, Usage};
use crate::CliError;

/// 跑一次导入（`--yes` 决定写不写）。返回要打印的 JSON。
pub(crate) fn run(args: &Args, store: &mut Store) -> Result<Value, CliError> {
    let from = PathBuf::from(args.required("from")?);
    let write = args.options.contains_key("yes");
    let only = args.optional("work").map(str::to_string);
    let fallback = match args.optional("language") {
        None => WorkLanguage::Zh,
        Some(text) => WorkLanguage::parse(text)?,
    };

    // **两遍法**（评审：轻微 19）：第一遍只解析与校验，一份都不写；整批都读得进来，
    // 第二遍才统一写。否则第 2 份解析失败时第 1 份已经入库了，而帮助文本承诺的是
    // "一份成稿读不进来就整批停下（不许救一半）"。
    let mut parsed: Vec<(PathBuf, WorkDraft)> = Vec::new();
    let mut titles = Vec::new();
    for path in collect_drafts(&from)? {
        let text = std::fs::read_to_string(&path)?;
        // 坏成稿**当场报错**（带上是哪一格不合格）：不许"跳过它接着导下一本"——
        // 那会让作者以为全导进来了
        let draft = store::parse_work_json(&text)?;
        titles.push(draft.title.clone());
        if let Some(wanted) = &only {
            if &draft.title != wanted {
                continue;
            }
        }
        parsed.push((path, draft));
    }

    let mut drafts = Vec::new();
    for (path, draft) in &parsed {
        drafts.push(one(store, draft, path, write, fallback)?);
    }
    if drafts.is_empty() {
        return Err(Usage(format!(
            "--work 指的《{}》不在这一批成稿里（这一批是：{}）",
            only.unwrap_or_default(),
            titles.join("、")
        ))
        .into());
    }
    Ok(json!({
        "ok": true,
        "command": "import",
        "wrote": write,
        "fallback_language": fallback.as_str(),
        "count": drafts.len(),
        "drafts": drafts,
    }))
}

/// 一本成稿：算账 + 对账；`write` 为真才真写库。
fn one(
    store: &mut Store,
    draft: &WorkDraft,
    path: &Path,
    write: bool,
    fallback: WorkLanguage,
) -> Result<Value, CliError> {
    // 成稿里记了语言就用它（更老的成稿没有这一格 → 用 --language，默认中文）
    let language = draft.language.unwrap_or(fallback);
    let stamp = stamp_for(path, &draft.title);
    let mut item = json!({
        "file": path.display().to_string(),
        "title": draft.title,
        "kind": draft.kind.as_str(),
        "language": language.as_str(),
        "naming": draft.naming.map(|style| style.as_str()),
        "scale": draft.scale(),
        // 库里已经有一本同名的（导入会**再建一本**，绝不覆盖）
        "same_title_in_library": same_title(store, &draft.title)?,
        "manifest": match &stamp {
            Some(stamp) => json!({ "book": stamp.title, "mismatches": draft.compare(stamp) }),
            None => Value::Null,
        },
    });
    if write {
        let report = store.import_draft(draft, language, &path.display().to_string())?;
        // 写进去之后再对一遍账——这一遍读的是**库里的数**，不是成稿里的数
        item["imported"] = json!({
            "work_id": report.work_id,
            "scale": report.scale,
            "mismatches": match &stamp {
                Some(stamp) => serde_json::to_value(report.scale.compare(stamp, &report.fingerprint))
                    .unwrap_or(Value::Null),
                None => json!([]),
            },
        });
    }
    Ok(item)
}

/// 要导入的成稿文件：给了文件就用它，给了目录就按包里的布局找。
fn collect_drafts(from: &Path) -> Result<Vec<PathBuf>, CliError> {
    if std::fs::metadata(from)?.is_file() {
        return Ok(vec![from.to_path_buf()]);
    }
    let found = store::find_drafts(from);
    if found.is_empty() {
        return Err(Usage(format!(
            "--from 指的位置里没有成稿：{}（备份包目录里应当是 成稿/<书名>/work.json；\
             也可以直接指到那份 work.json）",
            from.display()
        ))
        .into());
    }
    Ok(found)
}

/// 这一份成稿对应的清单条目（不是备份包就 `None`：那就只导入、不对账）。
fn stamp_for(path: &Path, title: &str) -> Option<WorkStamp> {
    store::manifest_near(path)?
        .1
        .works
        .into_iter()
        .find(|work| work.title == title)
}

/// 书架上同名作品有几本（只用来说明"会再建一本"，不拦）。
fn same_title(store: &Store, title: &str) -> Result<i64, CliError> {
    Ok(store.shelf()?.into_iter().filter(|entry| entry.work.title == title).count() as i64)
}
