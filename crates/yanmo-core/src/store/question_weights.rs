//! 模板权重：把作者的处置**教给同类模板**（偏好学习闭环的存储侧）。
//!
//! 两条纪律：
//!
//! 1. **学习和它记录的那次动作在同一个事务里**——做了处置却没学成、或没处置却学了，
//!    都会让"越用越懂你"变成"越用越莫名其妙"；
//! 2. **没有同类就不教**：作者自己写的问题、模块提交的问题没有模板键，
//!    闭环直接跳过（这不是失败，是"没有同类"）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use super::Store;
use crate::error::Result;
use crate::gravity::{apply, FeedbackSignal, PreferenceParams, TemplateWeight};
use crate::time::now_millis;

/// 一行学习记录：哪个模板、学成什么样（界面与演练台读它）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TemplateLearning {
    pub template_key: String,
    #[serde(flatten)]
    pub weight: TemplateWeight,
}

/// 一个模板学成什么样；没学过 = 默认（权重 1、启用、没有样本）。
pub(super) fn read_weight(conn: &Connection, key: &str) -> Result<TemplateWeight> {
    let found = conn
        .query_row(
            "SELECT weight, enabled, positives, negatives FROM question_template_weights
              WHERE template_key = ?1",
            params![key],
            |r| {
                Ok(TemplateWeight {
                    weight: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    positives: r.get(2)?,
                    negatives: r.get(3)?,
                })
            },
        )
        .optional()?;
    Ok(found.unwrap_or_default())
}

/// 写下新的学习状态（upsert：一行一个模板）。
fn write_weight(
    conn: &Connection,
    key: &str,
    weight: &TemplateWeight,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO question_template_weights(template_key, weight, enabled, positives, negatives,
                                               updated_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(template_key) DO UPDATE SET
             weight     = excluded.weight,
             enabled    = excluded.enabled,
             positives  = excluded.positives,
             negatives  = excluded.negatives,
             updated_at = excluded.updated_at",
        params![key, weight.weight, i64::from(weight.enabled), weight.positives, weight.negatives, now],
    )?;
    Ok(())
}

/// 把一条信号喂给同类模板（**调用方的事务里**调用，与那次动作同生共死）。
pub(super) fn learn_in(
    conn: &Connection,
    template_key: &str,
    signal: FeedbackSignal,
    now: i64,
) -> Result<()> {
    if template_key.is_empty() {
        return Ok(()); // 没有同类可教
    }
    let next = apply(&read_weight(conn, template_key)?, signal, &PreferenceParams::default());
    write_weight(conn, template_key, &next, now)
}

impl Store {
    /// 一个模板现在的学习状态（没学过就是默认）。
    pub fn template_weight(&self, key: &str) -> Result<TemplateWeight> {
        read_weight(&self.conn, key)
    }

    /// 学过的全部模板（按权重降序、同权按键）——界面说"它都学到了什么"、演练台核对着都靠它。
    pub fn template_weights(&self) -> Result<Vec<TemplateLearning>> {
        let mut stmt = self.conn.prepare(
            "SELECT template_key, weight, enabled, positives, negatives
               FROM question_template_weights
              ORDER BY weight DESC, template_key",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TemplateLearning {
                template_key: r.get(0)?,
                weight: TemplateWeight {
                    weight: r.get(1)?,
                    enabled: r.get::<_, i64>(2)? != 0,
                    positives: r.get(3)?,
                    negatives: r.get(4)?,
                },
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 已经静音的**类别**（模板）——界面要能看见它们、也能一键解除。
    ///
    /// 与"按来源静音"是两把不同的闸：**信不过这一类问题**（静音模板）vs **这个模块太吵**
    /// （静音来源）。两者都必须是"看得见的开关"，不然就成了只进不出。
    pub fn muted_templates(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT template_key FROM question_template_weights
              WHERE enabled = 0 ORDER BY template_key",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// **解除某一类的静音**：重新启用，并把权重放回中性（历史计数留着——那是"教过什么"的记录）。
    ///
    /// 与「某张卡从静音态回到待问」是两件事：那个是**卡的状态**，这个是**这一类的开关**。
    /// 没有这条出口，"静音"就成了只进不出的死路——产品口径明确要求它可撤销。
    pub fn unmute_template(&mut self, template_key: &str) -> Result<TemplateWeight> {
        let current = read_weight(&self.conn, template_key)?;
        let now = now_millis();
        let next = TemplateWeight {
            weight: 1.0,
            enabled: true,
            positives: current.positives,
            negatives: current.negatives,
        };
        let tx = self.conn.transaction()?;
        write_weight(&tx, template_key, &next, now)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "question_template_weights",
            0,
            "unmute",
            json!({ "template_key": template_key, "weight": next.weight }),
        )?;
        tx.commit()?;
        Ok(next)
    }

    /// 说「这个问题好」：**不动状态**——评价与处置是两件事，可以叠加
    /// （记完评价照旧延后 / 舍弃，卡的状态一个字节都不变）。
    ///
    /// 留一条 `praise` 事件，并把强正反馈喂给同类模板。
    pub fn praise_question_card(&mut self, card_id: i64, trigger: &str) -> Result<()> {
        let card = self.question_card(card_id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        Self::record_in(
            &self.device_id,
            &tx,
            "fragments",
            card_id,
            "praise",
            json!({
                "from": card.state.as_str(),
                "to": card.state.as_str(),
                "trigger": trigger,
                "template_key": card.template_key,
            }),
        )?;
        learn_in(&tx, &card.template_key, FeedbackSignal::Praised, now)?;
        tx.commit()?;
        Ok(())
    }
}
