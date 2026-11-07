//! 坏库打捞：把快照/库文件里**还能读出来的表与行**搬进一个新库。
//!
//! 与 `restore` 的分工：
//! - `restore` 换的是**备份包/完整库**（健康的那一份）；
//! - `salvage` 干的是**没有健康备份、但旧库还能读出一点**的那一档——
//!   它不改旧库、不覆盖任何文件，只往一个新路径写一份"能救回多少算多少"的库，
//!   然后把"哪张表整张跳过 / 哪张表只捞回几行 / 新库体检"如实报出来。
//!
//! 铁律：默认只打印计划，`--yes` 才动手；源库只读打开；目标必须不存在。

use std::path::Path;

use serde_json::{json, Value};

use yanmo_core::store::salvage_database;

use crate::args::Args;
use crate::CliError;

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

pub(crate) fn run(args: &Args) -> Result<Value, CliError> {
    let from = Path::new(args.required("from")?);
    let to = Path::new(args.required("to")?);

    if !from.is_file() {
        return Err(CliError::Refused {
            code: "salvage.source_missing",
            detail: format!("源库不存在：{}", from.display()),
        });
    }
    if from == to {
        return Err(CliError::Refused {
            code: "salvage.same_path",
            detail: "源与目标不能是同一个文件".to_string(),
        });
    }
    if to.exists() {
        return Err(CliError::Refused {
            code: "salvage.target_exists",
            detail: format!("目标已存在（不覆盖）：{}", to.display()),
        });
    }

    let bytes = std::fs::metadata(from).map(|meta| meta.len()).unwrap_or(0);
    let header = std::fs::read(from).map(|data| data[..data.len().min(16)].to_vec()).unwrap_or_default();
    let plan = json!({
        "source": from.display().to_string(),
        "target": to.display().to_string(),
        "source_bytes": bytes,
        "looks_like_sqlite": header == SQLITE_HEADER,
        "note": "源库只读；新库只写这一个文件，不覆盖、不合并",
    });

    if !args.flag("yes") {
        return Ok(json!({
            "ok": true,
            "command": "salvage",
            "dry_run": true,
            "plan": plan,
        }));
    }

    let report = salvage_database(from, to)?;
    Ok(json!({
        "ok": true,
        "command": "salvage",
        "dry_run": false,
        "report": report,
    }))
}
