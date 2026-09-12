//! 空缺路标：**删掉的章会留一条路标**，作者点「+」时问一次"要不要补写"。
//!
//! 两条分寸：
//!
//! 1. **主动问要有据**：只有"回收站里还躺着那一章"才算有据——知道它属于哪一层、第几号、
//!    几点几分删的、旧稿多少字。没有据的空档（改名造成的、旧稿已被彻底删除）**不主动问**，
//!    留给交付前的汇总检查（那一步按编号算，全面但不打扰）。
//! 2. **答复只记一份**：`settings` 里只记"问过没问过"，**空缺本身从数据算**——
//!    不另存一份会跟节点表打架的真相。
//!
//! 答复是个三态机（都在 [`Store::answer_gap`] 一处实现）：
//! 没问过 → 问了选「稍后」→ 下次再问一次 → 又选「稍后」→ 自动降为「不用了」。
//! 免得变成"每次点 + 都弹一下"的纠缠。

use rusqlite::{params, OptionalExtension};

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::NodeKind;

/// 作者对一处空缺的答复。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapAnswer {
    /// 稍后再说：下次在同一层点「+」时再问一次
    Deferred,
    /// 不用了：不再主动问（交付前的汇总检查仍会列出来）
    Ignored,
}

impl GapAnswer {
    fn as_str(self) -> &'static str {
        match self {
            GapAnswer::Deferred => "deferred",
            GapAnswer::Ignored => "ignored",
        }
    }

    /// 从界面传来的取值认出答复：**未知取值明确报错，不猜**（与节点类型的认法一致）。
    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "deferred" => Ok(GapAnswer::Deferred),
            "ignored" => Ok(GapAnswer::Ignored),
            other => {
                Err(Error::invalid_with(codes::UNKNOWN_GAP_ANSWER, [("value", other.to_string())]))
            }
        }
    }
}

/// 一处**有据可查**的空缺。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterGap {
    /// 回收站里那一条（界面上的「去回收站看看」用它）
    pub node_id: i64,
    /// 缺在哪一层；`None` = 根级
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    /// 第几号
    pub serial: i64,
    /// 原来叫什么
    pub title: String,
    pub deleted_at: i64,
    /// 旧稿多少字（让作者知道"字还在"）
    pub word_count: i64,
}

impl Store {
    /// 这本书这一层**现在**该问的空缺（最靠前的那一处）。没得问就返回 `None`。
    pub fn gap_in_layer(&self, work_id: i64, parent_id: Option<i64>) -> Result<Option<ChapterGap>> {
        let candidates = self.layer_gaps(work_id, parent_id)?;
        for gap in candidates {
            let answer = self.gap_answer(parent_id, gap.serial)?;
            if answer != Some(GapAnswer::Ignored) {
                return Ok(Some(gap));
            }
        }
        Ok(None)
    }

    /// 记下作者对某处空缺的答复。
    ///
    /// **三态机就在这一处**：已经"稍后"过一次的，再来一次就直接降为「不用了」——
    /// 系统可以问两次，但不该第三次还问。
    pub fn answer_gap(&self, gap_node_id: i64, answer: GapAnswer) -> Result<()> {
        let (parent_id, serial) = self.gap_coordinates(gap_node_id)?;
        let previous = self.gap_answer(parent_id, serial)?;
        let next = match (previous, answer) {
            (Some(GapAnswer::Deferred), GapAnswer::Deferred) => GapAnswer::Ignored,
            (_, wanted) => wanted,
        };
        self.write_gap_answer(parent_id, serial, next)
    }

    /// 补写某一处空缺：在它原来的层、用原来的名字与位置，新建一个**空章**。
    ///
    /// 注意这是「补写」不是「恢复」：**旧稿仍留在回收站里**，想去捞走回收站那条路
    /// （两边都出现时的同名冲突由恢复流程兜住）。补完顺手把答复记录清掉——这个号不空了。
    pub fn fill_gap_chapter(&mut self, gap_node_id: i64) -> Result<i64> {
        let work_id = self.work_of_any(gap_node_id)?;
        let (parent, serial) = self.gap_coordinates(gap_node_id)?;
        let (title, order): (String, i64) = self
            .conn
            .query_row(
                "SELECT title, sort_order FROM nodes WHERE id = ?1",
                params![gap_node_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_NOT_TRASHED, [("node_id", gap_node_id.to_string())])
            })?;

        let created = self.create_node(work_id, parent, NodeKind::Chapter, &title)?;
        // 落回它原来那一带（复用"按原位锚回"那套：序号越界就夹到末尾）
        self.move_node(created, parent, order.max(0) as usize)?;
        self.remove_gap_answer(parent, serial)?;
        Ok(created)
    }

    /// 交付前的汇总检查用：**按编号算出全书所有空缺**（不只"有据可查"的那些）。
    ///
    /// 有删除记录的会带上"何时删的、旧稿多少字"；查不到的只有"缺第x章"。
    pub fn list_gaps(&self, work_id: i64) -> Result<Vec<ChapterGap>> {
        super::work::ensure_alive(&self.conn, work_id)?;
        let nodes = self.list_nodes(work_id)?;
        let mut parents: Vec<Option<i64>> = vec![None];
        for node in &nodes {
            if node.kind.accepts_children() {
                parents.push(Some(node.id));
            }
        }

        let mut out = Vec::new();
        for parent in parents {
            let live: Vec<i64> = nodes
                .iter()
                .filter(|node| node.parent_id == parent)
                .filter_map(|node| parse_serial(&node.title, node.kind))
                .collect();
            let Some(max) = live.iter().copied().max() else {
                continue; // 这一层一个号都没有：不算有"空缺"
            };
            let known = self.layer_gaps(work_id, parent)?;
            for serial in 1..=max {
                if live.contains(&serial) {
                    continue;
                }
                match known.iter().find(|gap| gap.serial == serial) {
                    Some(gap) => out.push(gap.clone()),
                    None => out.push(ChapterGap {
                        node_id: 0,
                        parent_id: parent,
                        parent_title: self.layer_title(parent)?,
                        serial,
                        // 核心只报**编号**："缺第几章"那句话由界面按当前语言拼
                        // （见界面字典的 `gap.missing_chapter`）
                        title: String::new(),
                        deleted_at: 0,
                        word_count: 0,
                    }),
                }
            }
        }
        Ok(out)
    }

    /// 某一层里"有据可查"的空缺：被软删的章，且它的号**现在没人用**。
    ///
    /// 号已经被占着（作者自己改名补上了）就不算空缺——那种情形由"恢复时的同名冲突"去管。
    fn layer_gaps(&self, work_id: i64, parent_id: Option<i64>) -> Result<Vec<ChapterGap>> {
        let parent_title = self.layer_title(parent_id)?;
        let mut live_numbers: Vec<i64> = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT title FROM nodes
              WHERE work_id = ?1 AND parent_id IS ?2 AND node_kind = ?3 AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map(params![work_id, parent_id, NodeKind::Chapter.as_str()], |r| {
            r.get::<_, String>(0)
        })?;
        for row in rows {
            if let Some(serial) = parse_serial(&row?, NodeKind::Chapter) {
                live_numbers.push(serial);
            }
        }

        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.parent_id, n.title, n.deleted_at,
                    (SELECT word_count FROM nodes k WHERE k.id = n.id)
               FROM nodes n
              WHERE n.work_id = ?1 AND n.deleted_at IS NOT NULL AND n.parent_id IS ?2
                AND n.node_kind = ?3
              ORDER BY n.sort_order, n.id",
        )?;
        let rows = stmt.query_map(params![work_id, parent_id, NodeKind::Chapter.as_str()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (node_id, parent, title, deleted_at, word_count) = row?;
            let Some(serial) = parse_serial(&title, NodeKind::Chapter) else {
                continue; // 原来叫「序章」这种：没有"第x章"可补，不主动问
            };
            if live_numbers.contains(&serial) {
                continue;
            }
            out.push(ChapterGap {
                node_id,
                parent_id: parent,
                parent_title: parent_title.clone(),
                serial,
                title,
                deleted_at,
                word_count,
            });
        }
        Ok(out)
    }

    /// 空缺所属的层与号（答复记录按这两个值归档）。
    fn gap_coordinates(&self, gap_node_id: i64) -> Result<(Option<i64>, i64)> {
        let (parent, title): (Option<i64>, String) = self
            .conn
            .query_row(
                "SELECT parent_id, title FROM nodes WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![gap_node_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_NOT_TRASHED, [("node_id", gap_node_id.to_string())])
            })?;
        let serial = parse_serial(&title, NodeKind::Chapter)
            .ok_or_else(|| Error::invalid_with(codes::NODE_GAP_NO_SERIAL, [("title", title)]))?;
        Ok((parent, serial))
    }

    fn gap_answer(&self, parent_id: Option<i64>, serial: i64) -> Result<Option<GapAnswer>> {
        let stored = self.read_gap_answers()?;
        Ok(stored
            .get(&gap_key(parent_id, serial))
            .and_then(|value| GapAnswer::parse(value).ok()))
    }

    fn write_gap_answer(
        &self,
        parent_id: Option<i64>,
        serial: i64,
        answer: GapAnswer,
    ) -> Result<()> {
        let mut stored = self.read_gap_answers()?;
        stored.insert(gap_key(parent_id, serial), answer.as_str().to_string());
        let json = serde_json::to_string(&stored).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![GAP_ANSWERS_KEY, json, crate::time::now_millis()],
        )?;
        Ok(())
    }

    /// 清掉某处空缺的答复记录（补上了就不必再记着"问过没问过"）。
    fn remove_gap_answer(&self, parent_id: Option<i64>, serial: i64) -> Result<()> {
        let mut stored = self.read_gap_answers()?;
        if stored.remove(&gap_key(parent_id, serial)).is_some() {
            let json = serde_json::to_string(&stored).unwrap_or_default();
            self.conn.execute(
                "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
                params![GAP_ANSWERS_KEY, json, crate::time::now_millis()],
            )?;
        }
        Ok(())
    }

    fn read_gap_answers(&self) -> Result<std::collections::BTreeMap<String, String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![GAP_ANSWERS_KEY],
                |r| r.get(0),
            )
            .optional()?;
        // 记录坏了就当没问过：宁可多问一次，也不要漏掉一处该补的章
        Ok(raw
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    fn layer_title(&self, parent_id: Option<i64>) -> Result<Option<String>> {
        let Some(id) = parent_id else {
            return Ok(None);
        };
        Ok(Some(
            self.conn
                .query_row("SELECT title FROM nodes WHERE id = ?1", params![id], |r| r.get(0))
                .optional()?
                .unwrap_or_default(),
        ))
    }
}

/// 答复记录的键：**按层 + 号**（同一号在不同卷里互不影响）。
fn gap_key(parent_id: Option<i64>, serial: i64) -> String {
    format!("{}:{serial}", parent_id.unwrap_or(0))
}

/// 答复记录在 `settings` 里的键。
const GAP_ANSWERS_KEY: &str = "editor.chapter_gaps";

/// 从标题里认出编号：与建节点时同一套规矩（阿拉伯 + 中文数字，全角半角都按原样）。
pub(super) fn parse_serial(title: &str, kind: NodeKind) -> Option<i64> {
    super::node_edit::parse_serial(title, kind)
}
