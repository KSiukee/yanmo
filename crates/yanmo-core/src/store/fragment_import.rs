//! 外部文本导入：**粘贴一段 / 选一个 txt·md → 碎片池**。
//!
//! 三条硬规矩：
//!
//! 1. **带来源**：每批记 `label`（文件路径 / 「粘贴」）与 `source`（对外一律 `pasted`——
//!    "这段字不是在本软件里敲的"是事实，不能冒充 `typed`）；
//! 2. **按批可撤**：一批是一笔账（`import_batches`），撤只软删这一批碎片，**不碰正文**；
//! 3. **重复不产生副本**：同一条正文（逐字相同）已经在这个作品里，就不再加一份。
//!
//! 分批规则（空行 / 每行 / 整段）由 [`split_import_text`] 决定，**导入前先在界面上预览**。

use std::collections::HashSet;

use serde::Serialize;

use super::{work_alive, Store};
use crate::error::{codes, Error, Result};
use crate::model::FragmentKind;
use crate::text::content_hash;
use crate::time::now_millis;

/// 一次导入最多收多少字（界面不把几十万字一次性灌进来）。
pub const IMPORT_MAX_CHARS: usize = 2_000_000;

/// 怎么把一段文本切成条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitMode {
    /// 空行分段（备忘录最常见的样子）
    BlankLine,
    /// 每个非空行一条
    Line,
    /// 整段一条
    Whole,
}

impl SplitMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "line" => SplitMode::Line,
            "whole" => SplitMode::Whole,
            _ => SplitMode::BlankLine,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            SplitMode::BlankLine => "blank",
            SplitMode::Line => "line",
            SplitMode::Whole => "whole",
        }
    }
}

/// 把外部文本切成条目：去 BOM、归一换行、丢空条目。
///
/// **不替作者改字**：只 trim 首尾空白，不改标点、不合并行内空白。
pub fn split_import_text(text: &str, mode: &str) -> Result<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let chars = text.chars().count();
    if chars > IMPORT_MAX_CHARS {
        return Err(Error::invalid_with(
            codes::IMPORT_TEXT_TOO_LARGE,
            [
                ("chars", chars.to_string()),
                ("max", IMPORT_MAX_CHARS.to_string()),
            ],
        ));
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut items: Vec<String> = Vec::new();
    match SplitMode::parse(mode) {
        SplitMode::Line => {
            for line in normalized.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    items.push(line.to_string());
                }
            }
        }
        SplitMode::Whole => {
            let whole = normalized.trim();
            if !whole.is_empty() {
                items.push(whole.to_string());
            }
        }
        SplitMode::BlankLine => {
            let mut current: Vec<&str> = Vec::new();
            for line in normalized.lines() {
                if line.trim().is_empty() {
                    flush(&mut current, &mut items);
                } else {
                    current.push(line);
                }
            }
            flush(&mut current, &mut items);
        }
    }
    if items.is_empty() {
        return Err(Error::invalid(codes::IMPORT_TEXT_EMPTY));
    }
    Ok(items)
}

fn flush(current: &mut Vec<&str>, items: &mut Vec<String>) {
    if current.is_empty() {
        return;
    }
    let body = current.join("\n").trim().to_string();
    current.clear();
    if !body.is_empty() {
        items.push(body);
    }
}

/// 一批导入的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportOutcome {
    /// 这一批的账本号（全是重复时为空串——没有可撤的东西）
    pub batch_id: String,
    pub imported: usize,
    pub skipped: usize,
    pub total: usize,
}

/// 导入历史里的一批。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportBatch {
    pub id: String,
    pub label: String,
    pub source: String,
    pub kind: String,
    pub count: i64,
    pub created_at: i64,
    pub undone: bool,
}

impl Store {
    /// 导入一批碎片：带来源、按内容去重、记一笔批次账。
    ///
    /// 只写 `fragments` / `import_batches` / `op_log`——**一个字都不碰正文**。
    pub fn import_fragments(
        &mut self,
        work_id: i64,
        kind: FragmentKind,
        label: &str,
        source: &str,
        items: &[String],
    ) -> Result<ImportOutcome> {
        if items.is_empty() {
            return Err(Error::invalid(codes::IMPORT_TEXT_EMPTY));
        }
        if !kind.jotted() {
            return Err(Error::invalid_with(
                codes::FRAGMENT_KIND_NOT_JOTTED,
                [("value", kind.as_str().to_string())],
            ));
        }
        // 来源走与"记一条"同一个闭集：认不出来的当场拒，不假装
        let source = crate::model::InputSource::parse(source)?.as_str();
        let now = now_millis();
        let tx = self.conn.transaction()?;
        if !work_alive(&tx, work_id)? {
            return Err(Error::invalid_with(
                codes::WORK_GONE,
                [("work_id", work_id.to_string())],
            ));
        }

        // 去重：同一作品里**逐字相同**的现有碎片不再加一份
        let mut existing: HashSet<String> = HashSet::new();
        {
            let mut stmt =
                tx.prepare("SELECT body FROM fragments WHERE work_id = ?1 AND deleted_at IS NULL")?;
            let rows = stmt.query_map([work_id], |row| row.get::<_, String>(0))?;
            for row in rows {
                existing.insert(row?.trim().to_string());
            }
        }

        let mut fresh: Vec<String> = Vec::new();
        let mut skipped = 0usize;
        for item in items {
            let body = item.trim();
            if body.is_empty() {
                continue;
            }
            if existing.contains(body) || fresh.iter().any(|kept| kept == body) {
                skipped += 1;
            } else {
                fresh.push(body.to_string());
            }
        }

        let total = items.len();
        if fresh.is_empty() {
            return Ok(ImportOutcome {
                batch_id: String::new(),
                imported: 0,
                skipped,
                total,
            });
        }

        let fingerprint = content_hash(&format!(
            "{work_id}\u{1f}{}\u{1f}{label}\u{1f}{}",
            kind.as_str(),
            fresh.join("\u{1e}")
        ));
        let batch_id = format!("{fingerprint}-{now}");
        tx.execute(
            "INSERT INTO import_batches(id, work_id, kind, label, source, count, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                batch_id,
                work_id,
                kind.as_str(),
                label,
                source,
                fresh.len() as i64,
                now
            ],
        )?;

        for body in &fresh {
            tx.execute(
                "INSERT INTO fragments(work_id, frag_kind, body, source, status, importance,
                                       used_count, last_asked_at, linked, derived_from, auto_derived,
                                       template_key, created_at, updated_at, import_batch)
                 VALUES(?1, ?2, ?3, ?4, 'pending', 0.5, 0, NULL, '[]', NULL, 0, '', ?5, ?5, ?6)",
                rusqlite::params![work_id, kind.as_str(), body, source, now, batch_id],
            )?;
            let id = tx.last_insert_rowid();
            Self::record_in(
                &self.device_id,
                &tx,
                "fragments",
                id,
                "create",
                serde_json::json!({
                    "kind": kind.as_str(),
                    "source": source,
                    "trigger": "import",
                    "import_batch": batch_id,
                }),
            )?;
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "imports",
            0,
            "create",
            serde_json::json!({
                "batch": batch_id,
                "work_id": work_id,
                "count": fresh.len(),
                "label": label,
                "source": source,
            }),
        )?;
        tx.commit()?;
        Ok(ImportOutcome { batch_id, imported: fresh.len(), skipped, total })
    }

    /// 撤销一整批导入：只软删这一批碎片，**不碰正文**。返回撤掉的条数。
    pub fn undo_import(&mut self, batch_id: &str) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let row: Option<(i64, Option<i64>)> = tx
            .query_row(
                "SELECT work_id, undone_at FROM import_batches WHERE id = ?1",
                [batch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        let Some((work_id, undone_at)) = row else {
            return Err(Error::invalid_with(
                codes::IMPORT_BATCH_NOT_FOUND,
                [("batch_id", batch_id.to_string())],
            ));
        };
        if undone_at.is_some() {
            return Err(Error::invalid_with(
                codes::IMPORT_BATCH_NOT_FOUND,
                [("batch_id", batch_id.to_string())],
            ));
        }
        let now = now_millis();
        let removed = tx.execute(
            "UPDATE fragments SET deleted_at = ?1, updated_at = ?1
              WHERE import_batch = ?2 AND deleted_at IS NULL",
            rusqlite::params![now, batch_id],
        )? as i64;
        tx.execute(
            "UPDATE import_batches SET undone_at = ?1 WHERE id = ?2",
            rusqlite::params![now, batch_id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "imports",
            0,
            "undo",
            serde_json::json!({ "batch": batch_id, "work_id": work_id, "removed": removed }),
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// 这本书最近的导入批次（含已撤的，界面上标出来）。
    pub fn import_batches(&self, work_id: i64, limit: usize) -> Result<Vec<ImportBatch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, source, kind, count, created_at, undone_at
               FROM import_batches WHERE work_id = ?1
              ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![work_id, limit as i64], |row| {
            Ok(ImportBatch {
                id: row.get(0)?,
                label: row.get(1)?,
                source: row.get(2)?,
                kind: row.get(3)?,
                count: row.get(4)?,
                created_at: row.get(5)?,
                undone: row.get::<_, Option<i64>>(6)?.is_some(),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
