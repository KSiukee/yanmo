//! 五线推进度：**节点（章 / 节 / 场景卡）↔ 故事线**的标注。
//!
//! 表与理由见 `db::migrations_v19`。这里管三件事：读这本书的、读一段的、**换掉一段的**。
//!
//! 四条分寸：
//!
//! 1. **整份覆盖**：界面上那一格是一排勾选框，"这一章在推哪几条线"点一下就落定——
//!    交上来的就是那一段完整的清单，不做"加一条 / 减一条"两套接口（两套接口就有两处
//!    幂等语义要对齐，而这里的变化理由只有一个：作者改了标注）；
//! 2. **去重由调用点无关地保证**：同一章同一条线重复传只留一份（表上也有主键兜着）；
//! 3. **返回的永远是稳定序**：按 [`StoryLine::ALL`] 排（就是界面上那排勾选框的顺序），
//!    与输入顺序无关——作者拖乱/脚本乱给，读回来都是同一份；
//! 4. **空是常态**："这条线一条都没标"与"这一章没推进任何线"是两件事，
//!    读出来是空数组**只**说明前者（见 `model::story_line` 的口径）。
//!
//! **写坏了的码当场报错**（`model::StoryLine::parse`），不静默跳过：跳过等于把作者标过的那一条
//! 从界面上抹掉，而作者只会以为"我没标过"。

use std::collections::HashMap;

use rusqlite::params;
use serde_json::json;

use super::Store;
use crate::error::Result;
use crate::model::StoryLine;
use crate::time::now_millis;

/// 去重 + 排成**稳定序**（按 `ALL` 的顺序）。一遍扫完，不需要额外的集合。
fn in_stable_order(lines: &[StoryLine]) -> Vec<StoryLine> {
    StoryLine::ALL.into_iter().filter(|line| lines.contains(line)).collect()
}

impl Store {
    /// 这本书里**每个节点**推进的故事线（没标过的节点不会出现在结果里）。一次拿全。
    ///
    /// 大纲表要一屏摆完这本书：一条章一次往返会把"这本书有多少章"变成 N 次查询。
    pub fn node_lines(&self, work_id: i64) -> Result<HashMap<i64, Vec<StoryLine>>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.node_id, l.line
               FROM node_lines l
               JOIN nodes n ON n.id = l.node_id AND n.deleted_at IS NULL
              WHERE n.work_id = ?1
              ORDER BY l.node_id, l.line",
        )?;
        let rows = stmt.query_map(params![work_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out: HashMap<i64, Vec<StoryLine>> = HashMap::new();
        for row in rows {
            let (node_id, code) = row?;
            out.entry(node_id).or_default().push(StoryLine::parse(&code)?);
        }
        for lines in out.values_mut() {
            *lines = in_stable_order(lines);
        }
        Ok(out)
    }

    /// 一段的故事线（按 `ALL` 的顺序）。
    ///
    /// 没标过的段、以及不存在的段，都是**空数组**（不报错）——"作者还没标"不是故障。
    pub fn lines_of(&self, node_id: i64) -> Result<Vec<StoryLine>> {
        let mut stmt =
            self.conn.prepare("SELECT line FROM node_lines WHERE node_id = ?1 ORDER BY line")?;
        let rows = stmt.query_map(params![node_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(StoryLine::parse(&row?)?);
        }
        Ok(in_stable_order(&out))
    }

    /// 换掉一段的故事线（**整份覆盖**），返回库里真有的那一份（稳定序）。
    ///
    /// 传空数组＝**这一章清空**（不是"没动"）：作者把标过的线全取消，是正经的操作，
    /// 与"从没标过"在库里落到同一个状态（这一层不区分历史，要回头看历史有 op-log）。
    /// 与 [`Store::set_node_cast`](crate::store::Store::set_node_cast) 同一条纪律：
    /// 只动真变了的那几笔（没变的那条线保留它原来的 `created_at`——"什么时候标上的"不许被重写）。
    pub fn set_node_lines(
        &mut self,
        node_id: i64,
        lines: &[StoryLine],
        trigger: &str,
    ) -> Result<Vec<StoryLine>> {
        // 段在不在（软删的算不在）：往一段已经不存在的章上标线没有意义
        self.node_work(node_id)?;
        // 去重 + 稳定序：同一章同一条线重复传只留一份，返回的顺序与输入无关
        let wanted = in_stable_order(lines);

        let before: Vec<StoryLine> = {
            let mut stmt =
                self.conn.prepare("SELECT line FROM node_lines WHERE node_id = ?1 ORDER BY line")?;
            let rows = stmt.query_map(params![node_id], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(StoryLine::parse(&row?)?);
            }
            out
        };

        let now = now_millis();
        let tx = self.conn.transaction()?;
        let removed: Vec<StoryLine> =
            before.iter().copied().filter(|line| !wanted.contains(line)).collect();
        let added: Vec<StoryLine> =
            wanted.iter().copied().filter(|line| !before.contains(line)).collect();
        for line in &removed {
            tx.execute(
                "DELETE FROM node_lines WHERE node_id = ?1 AND line = ?2",
                params![node_id, line.as_str()],
            )?;
        }
        for line in &added {
            tx.execute(
                "INSERT INTO node_lines(node_id, line, created_at) VALUES(?1, ?2, ?3)",
                params![node_id, line.as_str(), now],
            )?;
        }
        if !added.is_empty() || !removed.is_empty() {
            Self::record_in(
                &self.device_id,
                &tx,
                "node_lines",
                node_id,
                "set",
                json!({
                    "added": added.len(),
                    "removed": removed.len(),
                    "total": wanted.len(),
                    "trigger": trigger,
                }),
            )?;
        }
        tx.commit()?;
        self.lines_of(node_id)
    }
}
