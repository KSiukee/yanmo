//! 镜像与主库的**一致性核对**：只报事实，不改任何东西。
//!
//! 三档结论：`ok`（一致）/ `different`（不一致，逐条列出差在哪）/ `missing`（还没写过镜像）。
//! 比对口径与镜像写入同一套编码（见 [`super::module_mirror`]），所以「同样是 1.5」不会因为
//! 一边是整数一边是浮点就报成不一致；行的物理顺序也**不参与**比较。

use std::collections::BTreeMap;

use serde::Serialize;

use super::module_data::{ensure_id, ModuleData};
use super::module_mirror::MirrorTable;
use crate::error::Result;

/// 一处不一致（`code` 稳定，界面按它取句子；`detail` 是表名）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MirrorDifference {
    /// `missing_table`（主库有、镜像没有）/ `stray_table`（镜像有、主库没有）/
    /// `columns`（列对不上）/ `rows`（行数对不上）/ `content`（行数相同但内容不同）。
    pub code: &'static str,
    pub detail: String,
}

/// 一次核对的结论（表数 / 行数说的是**主库**这一格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleMirrorCheck {
    pub module_id: String,
    /// `ok` / `different` / `missing`。
    pub state: &'static str,
    /// 镜像写下的时刻（没写过 = 0）。
    pub written_at: i64,
    pub tables: usize,
    pub rows: i64,
    pub differences: Vec<MirrorDifference>,
}

impl ModuleData {
    /// 核对这个模块的镜像与主库是否一致（只读）。
    pub fn check_mirror(&self, module_id: &str) -> Result<ModuleMirrorCheck> {
        ensure_id(module_id)?;
        let mut live: BTreeMap<String, MirrorTable> = BTreeMap::new();
        for name in self.tables_of(module_id)? {
            live.insert(name.clone(), self.read_table(&name)?);
        }
        let tables = live.len();
        let rows: i64 = live.values().map(|table| table.rows.len() as i64).sum();
        let mut differences = Vec::new();

        let Some(mirror) = self.read_mirror(module_id)? else {
            return Ok(ModuleMirrorCheck {
                module_id: module_id.to_string(),
                state: "missing",
                written_at: 0,
                tables,
                rows,
                differences,
            });
        };

        for table in &mirror.tables {
            match live.remove(&table.name) {
                None => {
                    differences.push(MirrorDifference { code: "stray_table", detail: table.name.clone() })
                }
                Some(live_table) => compare_table(&live_table, table, &mut differences),
            }
        }
        for name in live.keys() {
            differences.push(MirrorDifference { code: "missing_table", detail: name.clone() });
        }

        Ok(ModuleMirrorCheck {
            module_id: module_id.to_string(),
            state: if differences.is_empty() { "ok" } else { "different" },
            written_at: mirror.written_at,
            tables,
            rows,
            differences,
        })
    }
}

/// 比一张表：列 → 行数 → 内容（按规范化后的整行集合比，与行的物理顺序无关）。
fn compare_table(live: &MirrorTable, mirror: &MirrorTable, differences: &mut Vec<MirrorDifference>) {
    let live_columns: Vec<&str> = live.columns.iter().map(|column| column.name.as_str()).collect();
    let mirror_columns: Vec<&str> =
        mirror.columns.iter().map(|column| column.name.as_str()).collect();
    if live_columns != mirror_columns {
        differences.push(MirrorDifference { code: "columns", detail: live.name.clone() });
    } else if live.rows.len() != mirror.rows.len() {
        differences.push(MirrorDifference { code: "rows", detail: live.name.clone() });
    } else if canonical_rows(&live.rows) != canonical_rows(&mirror.rows) {
        differences.push(MirrorDifference { code: "content", detail: live.name.clone() });
    }
}

/// 行集合的规范形：每行序列化成字符串再排序——**与物理顺序无关**。
fn canonical_rows(rows: &[Vec<serde_json::Value>]) -> Vec<String> {
    let mut rendered: Vec<String> =
        rows.iter().map(|row| serde_json::to_string(row).unwrap_or_default()).collect();
    rendered.sort();
    rendered
}
