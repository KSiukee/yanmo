//! SOP 的**规矩与存取**：写库前那一道校验（紧挨着写库那一道，同一条流水线：
//! 作者输入 → 校验归一 → 落库留痕），以及三层的读取、改动留痕与回滚。
//!
//! 数据形状（类型 / 常量 / 默认模板 / 形状转换）在 [`super::sop`]——那边不碰库。
//! 与 `card` / `card_move`、`node` / `node_edit` 同一种分法：**同一件事的两个理由分开写**。

use std::collections::BTreeSet;

use rusqlite::{params, OptionalExtension};

use super::sop::{ResolvedSop, Sop, SopAction, SopCheck, SopRevision, SopSource, SopStep};
use super::sop::{ACTION_ASIDE, MAX_ID_CHARS, MAX_MINUTES, MAX_STEPS, MAX_TEXT_CHARS};
use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::SideTab;
use crate::time::now_millis;

impl Sop {
    /// 一份"没设过"的 SOP（写它等于把这层的记录删掉）。
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// **校验并归一**（写库前那一道）：id 干净唯一、文字收边、时长在范围里、动作认得出来。
    ///
    /// 这里严（作者的输入错了要说清哪一项错），读回来那条路松（坏记录当没设过）——
    /// 与 `appearance` 同一条分工，别反过来。
    pub fn checked(self) -> Result<Self> {
        if self.steps.len() > MAX_STEPS {
            return Err(Error::invalid_with(
                codes::SOP_TOO_MANY_STEPS,
                [
                    ("max", MAX_STEPS.to_string()),
                    ("count", self.steps.len().to_string()),
                ],
            ));
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut steps = Vec::with_capacity(self.steps.len());
        for step in self.steps {
            let id = checked_id(&step.id)?;
            if !seen.insert(id.clone()) {
                return Err(Error::invalid_with(codes::SOP_ID_DUPLICATE, [("value", id)]));
            }
            let mut check_ids: BTreeSet<String> = BTreeSet::new();
            let mut checks = Vec::with_capacity(step.checks.len());
            for check in step.checks {
                let check_id = checked_id(&check.id)?;
                if !check_ids.insert(check_id.clone()) {
                    return Err(Error::invalid_with(codes::SOP_ID_DUPLICATE, [("value", check_id)]));
                }
                checks.push(SopCheck {
                    id: check_id,
                    text: tidy(clean_text(check.text))?,
                    done: check.done,
                });
            }
            steps.push(SopStep {
                id,
                name: tidy(clean_text(step.name))?,
                note: tidy(clean_text(step.note))?,
                minutes: checked_minutes(step.minutes)?,
                checks,
                action: checked_action(step.action)?,
                skipped: step.skipped,
            });
        }
        Ok(Self { steps })
    }

    /// 从库里读回来的那份**收拾一遍**：坏项丢坏项，不把整份 SOP 判死。
    ///
    /// 逐项宽：认不出的动作丢掉、时长越界当没设、id 重复只留头一个、超过上限截断、
    /// 文字收边去空。**坏 JSON 那一层更宽**——整份当没设过（[`Store::read_sop`]）。
    fn tidied(self) -> Self {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut steps = Vec::new();
        for step in self.steps.into_iter().take(MAX_STEPS) {
            let id = step.id.trim().to_string();
            if !id_ok(&id) || !seen.insert(id.clone()) {
                continue;
            }
            let mut check_seen: BTreeSet<String> = BTreeSet::new();
            let checks = step
                .checks
                .into_iter()
                .filter_map(|check| {
                    let check_id = check.id.trim().to_string();
                    if !id_ok(&check_id) || !check_seen.insert(check_id.clone()) {
                        return None;
                    }
                    Some(SopCheck {
                        id: check_id,
                        text: clean_text(check.text),
                        done: check.done,
                    })
                })
                .collect();
            steps.push(SopStep {
                id,
                name: clean_text(step.name),
                note: clean_text(step.note),
                minutes: step.minutes.filter(|value| (1..=MAX_MINUTES).contains(value)),
                checks,
                action: step.action.filter(|action| {
                    action.target.trim() == ACTION_ASIDE
                        && SideTab::parse(action.value.trim()).is_ok()
                }),
                skipped: step.skipped,
            });
        }
        Self { steps }
    }
}

/// 稳定 id 的规矩：非空、只允许字母数字与 `-` `_`、别太长。
///
/// 为什么收这么紧：它要拼成界面字典的键（`sop.step.<id>`）——点号、空格、引号
/// 都会把字典查崩，而那种崩是**静默**的（查不到就原样显示键名）。
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= MAX_ID_CHARS
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn checked_id(raw: &str) -> Result<String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err(Error::invalid(codes::SOP_ID_EMPTY));
    }
    if !id_ok(id) {
        return Err(Error::invalid_with(
            codes::SOP_ID_BAD,
            [("value", id.to_string()), ("max", MAX_ID_CHARS.to_string())],
        ));
    }
    Ok(id.to_string())
}

/// 收边：去首尾空白、空串当没设过。
fn clean_text(text: Option<String>) -> Option<String> {
    text.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 长度那道闸（超了要说清是哪个数超了）。
fn tidy(text: Option<String>) -> Result<Option<String>> {
    let Some(text) = text else { return Ok(None) };
    let chars = text.chars().count();
    if chars > MAX_TEXT_CHARS {
        return Err(Error::invalid_with(
            codes::SOP_TEXT_TOO_LONG,
            [("chars", chars.to_string()), ("max", MAX_TEXT_CHARS.to_string())],
        ));
    }
    Ok(Some(text))
}

/// 参考时长：要么没设过，要么落在 `1..=MAX_MINUTES`（0 与负数不是"没设过"，是写错了）。
fn checked_minutes(minutes: Option<i64>) -> Result<Option<i64>> {
    match minutes {
        None => Ok(None),
        Some(value) if (1..=MAX_MINUTES).contains(&value) => Ok(Some(value)),
        Some(value) => Err(Error::invalid_with(
            codes::SOP_MINUTES_BAD,
            [("value", value.to_string()), ("max", MAX_MINUTES.to_string())],
        )),
    }
}

/// 绑定动作：这一版只认 `aside`（值用 [`SideTab`] 的稳定码，认不出报它自己的码）。
fn checked_action(action: Option<SopAction>) -> Result<Option<SopAction>> {
    let Some(action) = action else { return Ok(None) };
    let target = action.target.trim();
    if target != ACTION_ASIDE {
        return Err(Error::invalid_with(
            codes::SOP_ACTION_UNKNOWN_TARGET,
            [("value", target.to_string())],
        ));
    }
    let value = action.value.trim();
    SideTab::parse(value)?;
    Ok(Some(SopAction {
        target: ACTION_ASIDE.to_string(),
        value: value.to_string(),
    }))
}

impl Store {
    /// 读 SOP：**书的覆盖 → 全局 → 内置那套**（`work_id` 给 `Some` 才看书的覆盖）。
    ///
    /// 返回的 `source` 就是"这份是从哪来的"——界面据此说「这份只属于这一篇」
    /// 或「你还没设过，这是内置的起手流程」。
    pub fn sop(&self, work_id: Option<i64>) -> Result<ResolvedSop> {
        let global = self.read_sop(None)?;
        if let Some(id) = work_id {
            if let Some(work) = self.read_sop(Some(id))? {
                return Ok(ResolvedSop::at(SopSource::Work, work));
            }
        }
        match global {
            Some(sop) => Ok(ResolvedSop::at(SopSource::Global, sop)),
            None => Ok(ResolvedSop::default()),
        }
    }

    /// 写这一层的 SOP（**整份替换**：给它什么就是什么——见模块说明里的"不覆盖 / 覆盖"）。
    ///
    /// 空清单 = 把这层的记录**删掉**（回到继承 / 内置），与 `appearance` 同一条规矩。
    /// 写与留痕**同一个事务**：留痕失败不能把已经落库的改动报成失败。
    pub fn set_sop(&mut self, work_id: Option<i64>, value: Sop) -> Result<()> {
        let checked = value.checked()?;
        let tx = self.conn.transaction()?;
        write_sop(&tx, work_id, &checked)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id.unwrap_or(0),
            "set_sop",
            serde_json::to_value(&checked).unwrap_or(serde_json::Value::Null),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 清掉这一层的 SOP（书的覆盖则是"回到继承全局"）——留痕照记，**历史不涂改**。
    pub fn reset_sop(&mut self, work_id: Option<i64>) -> Result<()> {
        let tx = self.conn.transaction()?;
        write_sop(&tx, work_id, &Sop::default())?;
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id.unwrap_or(0),
            "reset_sop",
            serde_json::json!({ "steps": [] }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 这一层的改动历史（**新的在前**）：`limit` 条 op-log，每条带当时那一整份 SOP。
    ///
    /// 两次记录一比就是"我改了什么"；`seq` 就是版本号。
    pub fn sop_history(&self, work_id: Option<i64>, limit: usize) -> Result<Vec<SopRevision>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, payload, created_at FROM op_log
              WHERE entity = 'settings' AND entity_id = ?1 AND op IN ('set_sop', 'reset_sop')
              ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![work_id.unwrap_or(0), limit as i64], |row| {
            let seq: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            let at: i64 = row.get(2)?;
            Ok((seq, payload, at))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, payload, at) = row?;
            let sop = serde_json::from_str::<Sop>(&payload)
                .map(Sop::tidied)
                .unwrap_or_default();
            out.push(SopRevision { seq, at, sop });
        }
        Ok(out)
    }

    /// 回滚到历史上某一次（**它本身是一次新改动**，所以还能再回滚回去）。
    ///
    /// 找不到那条记录（换了本书 / seq 不存在）报 `sop.revision_not_found`——
    /// 宁可说找不到，也不要把别的那层的流程盖上来。
    pub fn restore_sop(&mut self, work_id: Option<i64>, seq: i64) -> Result<()> {
        let payload: Option<String> = self
            .conn
            .query_row(
                "SELECT payload FROM op_log
                  WHERE seq = ?1 AND entity = 'settings' AND entity_id = ?2
                    AND op IN ('set_sop', 'reset_sop')",
                params![seq, work_id.unwrap_or(0)],
                |row| row.get(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Err(Error::invalid_with(
                codes::SOP_REVISION_NOT_FOUND,
                [("seq", seq.to_string())],
            ));
        };
        let sop = serde_json::from_str::<Sop>(&payload).map(Sop::tidied).unwrap_or_default();
        if sop.is_empty() {
            return self.reset_sop(work_id);
        }
        self.set_sop(work_id, sop)
    }

    /// 读一层原始记录：键不存在 / JSON 坏了 / 空清单 → 都算"没设过"（`None`）。
    ///
    /// 坏记录当没设过是这一层的家规：宁可回内置那套，也不要让一格坏 JSON 把面板卡住。
    fn read_sop(&self, work_id: Option<i64>) -> Result<Option<Sop>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![sop_key(work_id)],
                |row| row.get(0),
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let Ok(parsed) = serde_json::from_str::<Sop>(&raw) else { return Ok(None) };
        let tidied = parsed.tidied();
        if tidied.is_empty() {
            return Ok(None);
        }
        Ok(Some(tidied))
    }
}

/// 写一层（空则删行——"没设过"与"设成空的"在数据上是同一件事）。
pub(super) fn write_sop(
    conn: &rusqlite::Connection,
    work_id: Option<i64>,
    value: &Sop,
) -> Result<()> {
    let key = sop_key(work_id);
    if value.is_empty() {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        return Ok(());
    }
    let json = serde_json::to_string(value).unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
        params![key, json, now_millis()],
    )?;
    Ok(())
}

/// 键的命名与 `appearance` 同一套：全局一个、每书一个（`work.{id}.sop`）。
fn sop_key(work_id: Option<i64>) -> String {
    match work_id {
        Some(id) => format!("work.{id}.sop"),
        None => "sop".to_string(),
    }
}
