//! **一整片格子**（从 Excel / WPS / 记事本粘进来的）一次写进库。
//!
//! 为什么它不是一个"批量调 [`Store::save_scene_fields`]"的循环：
//!
//! 1. **格子是逐格的，行不是**：粘进来的那一片里，有的行只落"一句话"、有的行只落"结果"。
//!    四格那条路是**整行覆盖**（界面一次给全四格），拿它循环会把"这一片没提的那几格"
//!    一起抹成空——那是静默毁稿，比报错难查得多。所以这里的字段写入是**逐格补丁**：
//!    只有这一片真给了的格才动，没给的格原样留着；
//! 2. **要么全落，要么一个字节都不落**：一片里有一格粘到了卷上（或者哪一段已经被删了），
//!    整次粘贴当场拒。留下"粘了一半"的表，作者根本看不出停在哪一行；
//! 3. **留痕按行记，不按格记**：一片 200 格全写 op-log 会把账本淹掉；而"这一章的这几格
//!    是粘进来的"按行一条已经说得清（`trigger` 由调用方给，界面传 `author`）。
//!
//! 一片的上限（[`MAX_PASTE_CELLS`]）不是防作者，是防"把整张表贴进来"这类误操作：
//! 一次写几千格既没有意义，也会把一次失误放大成整本书的改动。

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::{OutlineRow, Store};
use crate::error::{codes, Error, Result};
use crate::model::SceneField;
use crate::time::now_millis;

/// 一次粘贴最多落多少格。
///
/// 一本 200 章的书、把 5 个可填的列全填满也只有 1000 格——2000 是"正常用绝不会碰到、
/// 误把整张表贴进来会当场被拦住"的那个位置。
pub const MAX_PASTE_CELLS: usize = 2000;

/// 交上来的一格：**哪一段的哪一栏写什么**。`column` 是稳定码（`summary` / `pov` / …）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct OutlineCell {
    pub node_id: i64,
    pub column: String,
    pub value: String,
}

/// 粘进来的那一栏落在哪儿：一句话，还是四格里的某一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteColumn {
    Summary,
    Field(SceneField),
}

impl PasteColumn {
    /// 认得的只有这五个稳定码；别的（章名那一列、核心算出来的列、写错的码）一律当场拒。
    ///
    /// "哪些列能填"这件事界面上有一份（列开关那一栏），这里是**入库前的最后一关**：
    /// 界面漏了、或者将来多一条调用路径（命令行、模块），都拦在这里。
    fn parse(code: &str) -> Result<Self> {
        if code == "summary" {
            return Ok(PasteColumn::Summary);
        }
        SceneField::parse(code).map(PasteColumn::Field)
    }
}

/// 一段的四格补丁：`None` = 这一片没提这一格，**一个字都不动**。
#[derive(Debug, Default, Clone)]
struct FieldPatch {
    pov: Option<String>,
    goal: Option<String>,
    conflict: Option<String>,
    outcome: Option<String>,
}

impl FieldPatch {
    fn set(&mut self, field: SceneField, value: String) {
        match field {
            SceneField::Pov => self.pov = Some(value),
            SceneField::Goal => self.goal = Some(value),
            SceneField::Conflict => self.conflict = Some(value),
            SceneField::Outcome => self.outcome = Some(value),
        }
    }

    /// 补丁里真有的那几栏（留痕要记"这一行动了哪几栏"）。
    fn columns(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.pov.is_some() {
            names.push("pov");
        }
        if self.goal.is_some() {
            names.push("goal");
        }
        if self.conflict.is_some() {
            names.push("conflict");
        }
        if self.outcome.is_some() {
            names.push("outcome");
        }
        names
    }
}

/// 把这一片格子**一次写进库**，返回这本书最新那一屏大纲表。
///
/// 校验全部走完才开事务；事务提交后再读回库里那一份（回执是**库里真有的**，
/// 界面不靠自己回显）。
impl Store {
    pub fn save_outline_cells(
        &mut self,
        work_id: i64,
        cells: &[OutlineCell],
        trigger: &str,
    ) -> Result<Vec<OutlineRow>> {
        if cells.len() > MAX_PASTE_CELLS {
            return Err(Error::invalid_with(
                codes::OUTLINE_PASTE_TOO_BIG,
                [
                    ("cells", cells.len().to_string()),
                    ("max", MAX_PASTE_CELLS.to_string()),
                ],
            ));
        }

        // ── 第一步：全部解析 + 逐格核对（一个字都还没写） ──────────────────
        let mut summary: HashMap<i64, String> = HashMap::new();
        let mut patches: HashMap<i64, FieldPatch> = HashMap::new();
        for cell in cells {
            let column = PasteColumn::parse(&cell.column)?;
            // 这一段得属于这本书、还在、且能填（`node_work` 与 `has_fields` 各说一件事）
            if self.node_work(cell.node_id)? != work_id || !self.has_fields(cell.node_id)? {
                return Err(Error::invalid_with(
                    codes::NODE_NO_FIELDS,
                    [("node_id", cell.node_id.to_string())],
                ));
            }
            match column {
                // 一句话：**作者写了什么就是什么**（与 `set_node_summary` 同一条口径，不修剪）
                PasteColumn::Summary => {
                    summary.insert(cell.node_id, cell.value.clone());
                }
                // 四格：修剪后存（与四格那条路同一条口径）
                PasteColumn::Field(field) => {
                    patches
                        .entry(cell.node_id)
                        .or_default()
                        .set(field, cell.value.trim().to_string());
                }
            }
        }

        // ── 第二步：一个事务里写完（一句话 + 四格 + 每行一条留痕） ─────────
        let now = now_millis();
        let touched: BTreeSet<i64> = summary.keys().chain(patches.keys()).copied().collect();
        let tx = self.conn.transaction()?;
        for (node_id, text) in &summary {
            super::node_edit::set_summary_in(&tx, *node_id, text)?;
        }
        for (node_id, patch) in &patches {
            upsert_fields_in(&tx, *node_id, patch, now)?;
        }
        for node_id in &touched {
            let mut columns: Vec<&'static str> = Vec::new();
            if summary.contains_key(node_id) {
                columns.push("summary");
            }
            if let Some(patch) = patches.get(node_id) {
                columns.extend(patch.columns());
            }
            Store::record_in(
                &self.device_id,
                &tx,
                "nodes",
                *node_id,
                "outline_paste",
                json!({ "columns": columns, "trigger": trigger }),
            )?;
        }
        tx.commit()?;

        self.outline_rows(work_id)
    }
}

/// 逐格补齐一段的四格：**只动补丁里有的那几格**，别的原样留着。
///
/// 库里还没有这一行时按四个空串起手（"没填过"与"填成空"在这里是同一件事，
/// 与 `scene_cards` 卫星表的语义一致）。
fn upsert_fields_in(tx: &Connection, node_id: i64, patch: &FieldPatch, now: i64) -> Result<()> {
    let before: Option<(String, String, String, String)> = tx
        .query_row(
            "SELECT pov, goal, conflict, outcome FROM scene_cards WHERE node_id = ?1",
            params![node_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let (pov, goal, conflict, outcome) = before.unwrap_or_default();
    tx.execute(
        "INSERT INTO scene_cards(node_id, pov, goal, conflict, outcome, updated_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(node_id) DO UPDATE SET
            pov = excluded.pov, goal = excluded.goal, conflict = excluded.conflict,
            outcome = excluded.outcome, updated_at = excluded.updated_at",
        params![
            node_id,
            patch.pov.clone().unwrap_or(pov),
            patch.goal.clone().unwrap_or(goal),
            patch.conflict.clone().unwrap_or(conflict),
            patch.outcome.clone().unwrap_or(outcome),
            now
        ],
    )?;
    Ok(())
}
