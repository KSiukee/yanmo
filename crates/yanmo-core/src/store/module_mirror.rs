//! 模块数据的**开放格式镜像**：一份能被人读懂、能用来重建的 JSON。
//!
//! # 为什么要有它
//!
//! `modules.db` 是一个文件装了所有模块的数据——单点是它天生的代价。镜像就是这份代价的对冲：
//! 每个模块一份 JSON，落在数据目录里（跟库走），坏了能照着它把那一格重新搭起来。
//!
//! # 格式（版本 1）
//!
//! ```json
//! {
//!   "format": "yanmo-module-mirror",
//!   "version": 1,
//!   "module_id": "app.example.map",
//!   "schema_version": 2,
//!   "written_at": 1761000000000,
//!   "tables": [
//!     {
//!       "name": "mod_app.example.map_kv",
//!       "columns": [{"name": "key", "type": "TEXT", "not_null": false, "pk": 1}],
//!       "rows": [["theme", {"blob": "6461726b"}, 1761000000000]]
//!     }
//!   ]
//! }
//! ```
//!
//! 单元格按**运行时类型**记（不看声明类型）：`null` / 数字 / 字符串 / 二进制用
//! `{"blob":"<十六进制>"}`；非有限浮点（inf / -inf / nan）用 `{"real":"inf"}` 这类写法——
//! JSON 写不下它们，静默变成一个数才是最坏的结果。
//!
//! # 落点
//!
//! `<数据目录>/module-mirrors/<模块 id>.json`：**跟库走**（与本项目稿库镜像同一条口径），
//! 但**不与它混在一个目录**——那边是书的正文 `.md`，这边是模块数据；混在一起，
//! 「哪些文件能删」就说不清了。
//!
//! # 写是原子的
//!
//! 先写 `.tmp` 再 `rename`：断电时留下的要么是旧镜像、要么是新镜像，不会是一半。
//! 写之前**逐表核对归属**：只镜像这个模块自己的表，别人的一格都不带（与按模块导出同一条纪律）。

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use super::module_data::{ensure_id, quote, ModuleData};
use super::module_mirror_cells::encode_cell;
use super::module_namespace::{classify_table, TableOwner};
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 镜像文件的格式标记（写死；换格式要抬 `version`）。
pub const MIRROR_FORMAT: &str = "yanmo-module-mirror";
/// 镜像文件的格式版本。
pub const MIRROR_VERSION: u32 = 1;
/// 镜像目录名（在数据目录里，跟 `modules.db` 做邻居）。
pub const MIRROR_DIR: &str = "module-mirrors";

/// 一次「写镜像」的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleMirror {
    pub module_id: String,
    /// 写到了哪儿（绝对路径由打开 `modules.db` 的位置决定）。
    pub path: PathBuf,
    /// 文件字节数。
    pub bytes: u64,
    /// 带过去的表数。
    pub tables: usize,
    /// 行数合计。
    pub rows: i64,
    /// 写下的时刻（毫秒时间戳）。
    pub written_at: i64,
}

/// 一列的声明（重建时照它建表；索引 / 约束 / 触发器**不恢复**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct MirrorColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub not_null: bool,
    /// `PRAGMA table_info` 的 `pk` 序号（0 = 不是主键；复合主键是 1、2…）。
    pub pk: i64,
}

/// 一张表的结构与全部行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct MirrorTable {
    pub name: String,
    pub columns: Vec<MirrorColumn>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// 镜像文件的整体（**只在 core 内部流转**；对外给的是 [`ModuleMirror`] 那份小回执）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct MirrorFile {
    pub format: String,
    pub version: u32,
    pub module_id: String,
    pub schema_version: Option<i64>,
    pub written_at: i64,
    pub tables: Vec<MirrorTable>,
}

impl ModuleData {
    /// 一个模块的镜像文件路径（目录不存在也照常返回）。
    pub fn mirror_path(&self, module_id: &str) -> PathBuf {
        mirror_path(&self.path, module_id)
    }

    /// 把一个模块的命名空间**整份**写成 JSON 镜像（原子写）。
    ///
    /// 只带这个模块自己的表；一张都不带也照写（空镜像是有意义的：它证明「当时就是空的」）。
    ///
    /// 整次读（表清单 / 每张表的行 / 结构版本）包在**一个只读事务**里：模块正在写的时候
    /// 点「写镜像」，拿到的也是同一个时刻的样子，不会这张表新、那张表旧。
    pub fn mirror(&self, module_id: &str) -> Result<ModuleMirror> {
        ensure_id(module_id)?;
        let tx = self.conn.unchecked_transaction()?;
        let tables = Self::tables_of_in(&tx, module_id)?;
        let mut mirrored = Vec::with_capacity(tables.len());
        let mut rows_total: i64 = 0;
        for table in &tables {
            if !matches!(classify_table(table), TableOwner::Module { .. }) {
                return Err(Error::invalid_with(
                    codes::MODULE_MIRROR_INVALID,
                    [("detail", format!("table does not belong to any module: {table}"))],
                ));
            }
            let table = read_table_in(&tx, table)?;
            rows_total = rows_total.saturating_add(table.rows.len() as i64);
            mirrored.push(table);
        }
        let schema_version: Option<i64> = tx
            .query_row(
                "SELECT schema_version FROM module_schema WHERE module_id = ?1",
                [module_id],
                |row| row.get(0),
            )
            .optional()?;
        tx.commit()?;
        let file = MirrorFile {
            format: MIRROR_FORMAT.to_string(),
            version: MIRROR_VERSION,
            module_id: module_id.to_string(),
            schema_version,
            written_at: now_millis(),
            tables: mirrored,
        };
        let path = self.mirror_path(module_id);
        let bytes = write_mirror(&path, &file)?;
        Ok(ModuleMirror {
            module_id: module_id.to_string(),
            path,
            bytes,
            tables: file.tables.len(),
            rows: rows_total,
            written_at: file.written_at,
        })
    }

    /// 镜像过期（或读不出来）才重写一份；一致就什么都不做。
    ///
    /// 返回 `Some` = 这次真写了；`None` = 本来就跟主库一致。**一致性由
    /// [`ModuleData::check_mirror`](crate::store::ModuleData::check_mirror) 判断**——
    /// 不按时间猜，所以「跑完一轮但什么都没改」不会白白重写一份。
    pub fn refresh_mirror_if_stale(&self, module_id: &str) -> Result<Option<ModuleMirror>> {
        ensure_id(module_id)?;
        // 镜像坏了 / 读了半天读不出来：正是该重写的时候（这里不是错误，是判断的结果）
        let fresh = matches!(self.check_mirror(module_id), Ok(check) if check.state == "ok");
        if fresh {
            return Ok(None);
        }
        self.mirror(module_id).map(Some)
    }

    /// 收工/跑完之后：**只给变过的模块补镜像**（一致的不写）。
    ///
    /// 返回真正写了镜像的模块 id——空表示"全都没变，什么都没写"。
    /// 一个模块写失败不挡其他模块：镜像坏了不该把退出也拖住。
    pub fn refresh_mirrors_changed(&self) -> Result<Vec<String>> {
        let mut written = Vec::new();
        for module_id in self.module_ids()? {
            if let Ok(Some(_)) = self.refresh_mirror_if_stale(&module_id) {
                written.push(module_id);
            }
        }
        Ok(written)
    }

    /// 给**所有**有账的模块各写一份镜像（显式的「全部留一份」）。
    ///
    /// 「有账」＝库里有它的表，或 `module_schema` 里有它的行。中途任何一份写失败就整体返回
    /// 错误（并把是哪一步写不出来带在 `detail` 里）——这一动作本来就是"留底"，半套没意义。
    pub fn mirror_all(&self) -> Result<Vec<ModuleMirror>> {
        let mut receipts = Vec::new();
        for module_id in self.module_ids()? {
            receipts.push(self.mirror(&module_id)?);
        }
        Ok(receipts)
    }

    /// 库里有账的模块 id（有表，或 `module_schema` 里记过版本），按名字排序。
    fn module_ids(&self) -> Result<Vec<String>> {
        let mut ids = std::collections::BTreeSet::new();
        for table in Self::all_tables_in(&self.conn)? {
            if let TableOwner::Module { module_id, .. } = classify_table(&table) {
                ids.insert(module_id);
            }
        }
        let mut stmt = self.conn.prepare("SELECT module_id FROM module_schema")?;
        let recorded = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.extend(recorded);
        Ok(ids.into_iter().collect())
    }

    /// 读一份镜像（**只校验形状与归属**，不碰主库）。没写过返回 `Ok(None)`。
    pub(super) fn read_mirror(&self, module_id: &str) -> Result<Option<MirrorFile>> {
        ensure_id(module_id)?;
        read_mirror(&self.mirror_path(module_id), module_id)
    }

    /// 读取一张模块表的结构与全部行（按 `sqlite_master` 里的顺序取列）。
    pub(super) fn read_table(&self, table: &str) -> Result<MirrorTable> {
        read_table_in(&self.conn, table)
    }
}

/// 读一张表（连接由调用方给：镜像写入要在事务里读，核对则不必）。
fn read_table_in(conn: &rusqlite::Connection, table: &str) -> Result<MirrorTable> {
    let columns = columns_of(conn, table)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM {}",
        columns.iter().map(|column| quote(&column.name)).collect::<Vec<_>>().join(", "),
        quote(table)
    ))?;
    let width = columns.len();
    let mut rows = Vec::new();
    let mut cursor = stmt.query([])?;
    while let Some(row) = cursor.next()? {
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            cells.push(encode_cell(row.get_ref(index)?)?);
        }
        rows.push(cells);
    }
    Ok(MirrorTable { name: table.to_string(), columns, rows })
}

/// 镜像文件路径：`<modules.db 的目录>/module-mirrors/<模块 id>.json`。
pub(super) fn mirror_path(db_path: &Path, module_id: &str) -> PathBuf {
    let dir = db_path.parent().map(|parent| parent.join(MIRROR_DIR)).unwrap_or_else(|| PathBuf::from(MIRROR_DIR));
    dir.join(format!("{module_id}.json"))
}

/// 原子写：先写 `.tmp`，再 `rename` 盖过去。返回文件字节数。
fn write_mirror(path: &Path, file: &MirrorFile) -> Result<u64> {
    let Some(dir) = path.parent() else {
        return Err(Error::invalid_with(codes::MODULE_MIRROR_FAILED, [("detail", "no parent dir".into())]));
    };
    std::fs::create_dir_all(dir).map_err(|error| {
        Error::invalid_with(codes::MODULE_MIRROR_FAILED, [("detail", error.to_string())])
    })?;
    let bytes = serde_json::to_vec_pretty(file).map_err(|error| {
        Error::invalid_with(codes::MODULE_MIRROR_FAILED, [("detail", error.to_string())])
    })?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, &bytes).map_err(|error| {
        Error::invalid_with(codes::MODULE_MIRROR_FAILED, [("detail", error.to_string())])
    })?;
    std::fs::rename(&temp, path).map_err(|error| {
        Error::invalid_with(codes::MODULE_MIRROR_FAILED, [("detail", error.to_string())])
    })?;
    Ok(bytes.len() as u64)
}

/// 读 + 形状校验（格式标记 / 版本 / 归属 / 行宽）。
fn read_mirror(path: &Path, module_id: &str) -> Result<Option<MirrorFile>> {
    let text = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error::invalid_with(
                codes::MODULE_MIRROR_FAILED,
                [("detail", error.to_string())],
            ))
        }
    };
    let file: MirrorFile = serde_json::from_slice(&text).map_err(|error| {
        Error::invalid_with(codes::MODULE_MIRROR_INVALID, [("detail", error.to_string())])
    })?;
    if file.format != MIRROR_FORMAT || file.version != MIRROR_VERSION {
        return Err(Error::invalid_with(
            codes::MODULE_MIRROR_INVALID,
            [("detail", format!("format {} v{}", file.format, file.version))],
        ));
    }
    if file.module_id != module_id {
        return Err(Error::invalid_with(
            codes::MODULE_MIRROR_INVALID,
            [("detail", format!("mirror belongs to {}", file.module_id))],
        ));
    }
    for table in &file.tables {
        let owned = matches!(
            classify_table(&table.name),
            TableOwner::Module { module_id: found, .. } if found == module_id
        );
        if !owned {
            return Err(Error::invalid_with(
                codes::MODULE_MIRROR_INVALID,
                [("detail", format!("table outside the namespace: {}", table.name))],
            ));
        }
        if table.columns.is_empty() {
            return Err(Error::invalid_with(
                codes::MODULE_MIRROR_INVALID,
                [("detail", format!("table has no column: {}", table.name))],
            ));
        }
        for row in &table.rows {
            if row.len() != table.columns.len() {
                return Err(Error::invalid_with(
                    codes::MODULE_MIRROR_INVALID,
                    [("detail", format!("row width mismatch: {}", table.name))],
                ));
            }
        }
    }
    Ok(Some(file))
}

/// `PRAGMA table_info` → 列声明（顺序即列序）。
fn columns_of(conn: &rusqlite::Connection, table: &str) -> Result<Vec<MirrorColumn>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote(table)))?;
    let columns = stmt
        .query_map([], |row| {
            Ok(MirrorColumn {
                name: row.get(1)?,
                kind: row.get(2)?,
                not_null: row.get::<_, i64>(3)? != 0,
                pk: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}
