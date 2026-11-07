// i18n-allow-file: 打捞报告是给命令行/排查看的诊断清单，不直接进界面（与 backup.rs 同一口径）
//! 坏库打捞：把**还能读出来的表与行**搬进一个新库。
//!
//! 分寸（这是"库已经坏了"时才走的路，宁可少捞也别二次伤害）：
//!
//! - **源库只读打开**（URI `mode=ro`）：打捞过程一个字节都不改源文件；
//! - **目标必须是新文件**：不覆盖、不合并，打捞失败最多留下一个半成品，删掉重来；
//! - **先整表、再逐行**：整表 `INSERT … SELECT` 失败（页坏了）就退到"按 rowid 一行一行搬"，
//!   坏掉的那几行跳过并计数——这就是"页级打捞只作兜底"里的兜底层；
//! - **结构以新库为准**：目标库由当前版本的迁移建好，只搬**两边同名的列**；
//!   老版本少一列、新版本多一列都不影响，缺的列走默认值；
//! - **FTS 最后重建**：倒排索引不搬（搬了也可能与正文对不上），从正文重建。
//!
//! 打捞不了的东西也如实报：哪张表整张跳过、哪张表只捞回几行、目标库体检结果。

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;

use super::Store;
use crate::error::{Error, Result};

/// 一张表的打捞结果。
#[derive(Debug, Clone, Serialize)]
pub struct SalvagedTable {
    pub name: String,
    /// 目标库里最终有多少行（整表成功或逐行补捞之后）
    pub rows: i64,
    /// `copied` = 整表搬成功；`partial` = 逐行补捞；`skipped` = 读不出来
    pub status: String,
    /// 补捞时第一条读不出来的原因（整表成功时是空串）——给真机排查用
    pub note: String,
}

/// 一份打捞报告。
#[derive(Debug, Clone, Serialize)]
pub struct SalvageReport {
    pub source: String,
    pub target: String,
    /// 源文件字节数（"到底是不是那个库"的第一眼凭据）
    pub source_bytes: u64,
    pub tables: Vec<SalvagedTable>,
    /// 从正文重建过的全文索引
    pub rebuilt_fts: Vec<String>,
    /// 目标库 `integrity_check` 的前几行（`ok` 才是干净）
    pub integrity: Vec<String>,
    /// 源库里读不到、或目标里没有对应表的那些名字（如实列出）
    pub unreadable: Vec<String>,
    /// 目标库里父行没搬回来的引用（外键检查；空 = 没有悬空引用）
    pub dangling: Vec<String>,
}

/// 把 `source` 里能读出来的东西搬进 `target`（`target` 必须不存在）。
pub fn salvage_database(source: &Path, target: &Path) -> Result<SalvageReport> {
    if !source.is_file() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("源库不存在：{}", source.display()),
        )));
    }
    if target.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("目标已存在（不覆盖）：{}", target.display()),
        )));
    }

    // 目标库：用当前版本的迁移建好结构，然后立刻关掉——接下来用一条可写连接把数据写进去
    let store = Store::open(target)?;
    drop(store);

    let source_bytes = std::fs::metadata(source).map(|meta| meta.len()).unwrap_or(0);
    // 目标是主库（可写，FTS 体检/重建要写权限）；源库用 URI 只读挂上来
    let conn = Connection::open_with_flags(
        target,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(Error::from)?;
    // 相对路径要先变绝对：URI 形式没有"当前目录"这个概念
    let source_abs = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    conn.execute("ATTACH DATABASE ?1 AS src", params![file_uri(&source_abs)])
        .map_err(Error::from)?;
    // 搬的时候按表名顺序来，父表不一定先到——先关外键，搬完再用 foreign_key_check 如实报
    conn.execute_batch("PRAGMA foreign_keys = OFF;").map_err(Error::from)?;

    let mut report = SalvageReport {
        source: source.display().to_string(),
        target: target.display().to_string(),
        source_bytes,
        tables: Vec::new(),
        rebuilt_fts: Vec::new(),
        integrity: Vec::new(),
        unreadable: Vec::new(),
        dangling: Vec::new(),
    };

    let source_tables = table_names(&conn, "src").unwrap_or_else(|_| {
        report.unreadable.push("（连 sqlite_master 都读不出来）".to_string());
        Vec::new()
    });
    let target_tables: BTreeSet<String> =
        table_names(&conn, "main").unwrap_or_default().into_iter().collect();

    for name in source_tables {
        // 全文索引的虚表与影子表不搬：最后从正文重建
        if is_fts_related(&name) || name.starts_with("sqlite_") {
            continue;
        }
        if !target_tables.contains(&name) {
            report.unreadable.push(format!("{name}（新库没有这张表）"));
            continue;
        }
        match copy_table(&conn, &name) {
            Ok((rows, status, note)) => {
                report.tables.push(SalvagedTable { name, rows, status, note })
            }
            Err(error) => report.unreadable.push(format!("{name}（{error}）")),
        }
    }

    rebuild_fts(&conn, &mut report);
    report.integrity = integrity(&conn);
    report.dangling = foreign_key_problems(&conn);
    Ok(report)
}

/// Windows 路径 → SQLite URI（只读）。只转 `file:` 需要的那几个字符，
/// 不依赖任何路径库；形如 `…/目录/库.db` 会变成 `file:///…/目录/库.db?mode=ro`
/// （`…` 处是绝对盘符；空格等字符按 URI 规则转义）。
fn file_uri(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    // Windows 的绝对路径可能带 `\\?\`（canonicalize 会给）：URI 不认这个前缀
    if let Some(rest) = text.strip_prefix("//?/") {
        text = rest.to_string();
    }
    if let Some(rest) = text.strip_prefix("//./") {
        text = rest.to_string();
    }
    let mut encoded = String::new();
    for ch in text.chars() {
        match ch {
            '%' => encoded.push_str("%25"),
            '?' => encoded.push_str("%3F"),
            '#' => encoded.push_str("%23"),
            ' ' => encoded.push_str("%20"),
            _ => encoded.push(ch),
        }
    }
    let body = encoded.trim_start_matches('/');
    format!("file:///{body}?mode=ro")
}

/// 源库 / 目标库里的普通表名（`sqlite_*` 也返回，由调用方过滤）。
fn table_names(conn: &Connection, schema: &str) -> Result<Vec<String>> {
    let sql = format!("SELECT name FROM {schema}.sqlite_master WHERE type = 'table' ORDER BY name");
    let mut statement = conn.prepare(&sql).map_err(Error::from)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(Error::from)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

fn is_fts_related(name: &str) -> bool {
    name.ends_with("_fts")
        || name.ends_with("_fts_data")
        || name.ends_with("_fts_idx")
        || name.ends_with("_fts_content")
        || name.ends_with("_fts_docsize")
        || name.ends_with("_fts_config")
}

/// 两边同名的列（顺序按目标表）。
fn common_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let target = columns(conn, "main", table)?;
    let source: BTreeSet<String> = columns(conn, "src", table)?.into_iter().collect();
    Ok(target.into_iter().filter(|name| source.contains(name)).collect())
}

fn columns(conn: &Connection, schema: &str, table: &str) -> Result<Vec<String>> {
    let sql = format!("SELECT name FROM pragma_table_info(?1, '{schema}')");
    let mut statement = conn.prepare(&sql).map_err(Error::from)?;
    let rows = statement
        .query_map([table], |row| row.get::<_, String>(0))
        .map_err(Error::from)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// 先整表搬；失败退到逐行（按 rowid）搬，坏行跳过。返回（目标行数, 状态）。
fn copy_table(conn: &Connection, table: &str) -> Result<(i64, String, String)> {
    let columns = common_columns(conn, table)?;
    if columns.is_empty() {
        return Ok((0, "skipped".to_string(), String::new()));
    }
    let list = columns.iter().map(|name| quoted(name)).collect::<Vec<_>>().join(", ");
    let table = quoted(table);
    let sql = format!(
        "INSERT OR REPLACE INTO main.{table} ({list}) SELECT {list} FROM src.{table}"
    );
    let whole_error = match conn.execute(&sql, []) {
        Ok(_) => {
            let rows = count_rows(conn, &table);
            return Ok((rows, "copied".to_string(), String::new()));
        }
        Err(error) => error.to_string(),
    };

    // 整表失败：按 rowid 逐行捞（只对 rowid 表有效；WITHOUT ROWID 的表只能整表）
    let mut ids = Vec::new();
    if let Ok(mut statement) = conn.prepare(&format!("SELECT rowid FROM src.{table}")) {
        if let Ok(rows) = statement.query_map([], |row| row.get::<_, i64>(0)) {
            ids = rows.filter_map(|row| row.ok()).collect();
        }
    }
    if ids.is_empty() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "整表读不出来，也没有 rowid 可逐行捞".to_string(),
        )));
    }
    let mut first_error = String::new();
    for id in ids {
        let sql = format!(
            "INSERT OR REPLACE INTO main.{table} ({list}) SELECT {list} FROM src.{table} WHERE rowid = ?1"
        );
        // 单个坏行读不出来就跳过；它不会拖垮同一张表的其它行
        if let Err(error) = conn.execute(&sql, params![id]) {
            if first_error.is_empty() {
                first_error = error.to_string();
            }
        }
    }
    let rows = count_rows(conn, &table);
    let note = if first_error.is_empty() { whole_error } else { first_error };
    Ok((rows, "partial".to_string(), note))
}

fn count_rows(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM main.{table}"), [], |row| row.get(0))
        .unwrap_or(0)
}

/// 全文索引从正文重建（搬倒排索引等于搬一份可能已对不上的影子）。
fn rebuild_fts(conn: &Connection, report: &mut SalvageReport) {
    let Ok(mut statement) = conn.prepare(
        "SELECT name FROM main.sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%'",
    ) else {
        return;
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return;
    };
    for name in rows.filter_map(|row| row.ok()) {
        // FTS5 的 rebuild：往同名隐藏列里插 'rebuild'
        let table = quoted(&name);
        let sql = format!("INSERT INTO main.{table} ({table}) VALUES ('rebuild')");
        if conn.execute(&sql, []).is_ok() {
            report.rebuilt_fts.push(name);
        } else {
            report.unreadable.push(format!("{name}（索引重建失败）"));
        }
    }
}

/// 搬完后的外键检查：父行没捞回来时，这里如实列出（不阻止打捞，但要让人知道）。
fn foreign_key_problems(conn: &Connection) -> Vec<String> {
    let Ok(mut statement) = conn.prepare("PRAGMA main.foreign_key_check") else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        let table: String = row.get(0)?;
        let rowid: Option<i64> = row.get(1)?;
        let parent: String = row.get(2)?;
        Ok(format!("{table} 的 rowid {rowid:?} 指向不存在的 {parent}"))
    }) else {
        return Vec::new();
    };
    rows.filter_map(|row| row.ok()).take(10).collect()
}

fn integrity(conn: &Connection) -> Vec<String> {
    let Ok(mut statement) = conn.prepare("PRAGMA main.integrity_check") else {
        return vec!["（体检读不出来）".to_string()];
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return vec!["（体检失败）".to_string()];
    };
    rows.filter_map(|row| row.ok()).take(5).collect()
}
