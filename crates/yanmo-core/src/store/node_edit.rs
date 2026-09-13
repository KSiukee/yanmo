//! 节点树的**编辑**：建 / 改名 / 移动 / 软删除。
//!
//! - 同级排序是**密集序号**（0,1,2…）：每次移动后重排，幂等、无空洞、结果可复现；
//! - 移动前做**成环检测**：不允许把节点移进自己的子孙——树坏掉比报错难查得多；
//! - 软删除连带整棵子树，正文与历史都留着（回收站与"删错了"的撤销靠它）；
//! - 读在 [`super::node`]，两边分开，改编辑不会牵动目录树查询。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::{Store, MAX_TREE_DEPTH};
use crate::error::{codes, Error, Result};
use crate::model::{NamingStyle, NodeKind};
use crate::time::now_millis;

/// 同父下的节点 id，按现有顺序。
fn sibling_ids(conn: &Connection, work_id: i64, parent_id: Option<i64>) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM nodes
         WHERE work_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL
         ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![work_id, parent_id], |r| r.get(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 把同父节点重排成密集序号；`moved` 指定要挪到 `index` 位置的节点。
///
/// 删除与恢复也要用它（删完收洞、恢复时按原位锚回），所以对同层的模块开放。
pub(super) fn renumber(
    conn: &Connection,
    work_id: i64,
    parent_id: Option<i64>,
    moved: Option<(i64, usize)>,
) -> Result<()> {
    let mut ids = sibling_ids(conn, work_id, parent_id)?;
    if let Some((id, index)) = moved {
        ids.retain(|x| *x != id);
        ids.insert(index.min(ids.len()), id);
    }
    for (position, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE nodes SET sort_order = ?1 WHERE id = ?2",
            params![position as i64, id],
        )?;
    }
    Ok(())
}

/// `candidate` 是否在 `ancestor` 的子树里。
fn is_descendant(conn: &Connection, candidate: i64, ancestor: i64) -> Result<bool> {
    let mut current = candidate;
    for _ in 0..MAX_TREE_DEPTH {
        let parent: Option<i64> = conn
            .query_row(
                "SELECT parent_id FROM nodes WHERE id = ?1",
                params![current],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        match parent {
            Some(p) if p == ancestor => return Ok(true),
            Some(p) => current = p,
            None => return Ok(false),
        }
    }
    Err(Error::invalid(codes::TREE_CYCLE_SUSPECTED))
}

/// 编号骨架的前后缀：`第 12 章` 里的「第」「章」。
// i18n-allow-begin: 这张表产出的是**会写进库的默认名**（作者的数据，可随时改），不是界面文案
fn naming_words(kind: NodeKind) -> (&'static str, &'static str) {
    match kind {
        NodeKind::Volume => ("第", "卷"),
        NodeKind::Chapter => ("第", "章"),
        NodeKind::Section => ("第", "节"),
        NodeKind::Piece => ("第", "篇"),
        // 场景卡没有"第…卡"这种骨架：整串都得是数，所以它压根不编号
        NodeKind::Scene => ("", ""),
    }
}
// i18n-allow-end

/// 新建节点时的**默认名模板**（`{$N}` 在显示/导出时按同层位置渲染）。
///
/// - 编号规则由作者在设置里选（`NamingStyle`），没选过就跟**作品类型**走；
/// - 选「不编号」或本来就是场景卡 → 给空标题，名字留给作者（界面按语言补占位显示）。
pub(super) fn template_for(kind: NodeKind, style: NamingStyle) -> String {
    if kind == NodeKind::Scene || style == NamingStyle::NoNumber {
        return String::new();
    }
    let (prefix, suffix) = naming_words(kind);
    format!("{prefix}{}{suffix}", style.counter())
}

/// 标题留空时的默认名 = **模板**。
///
/// 为什么不再"扫描同层取最大号 + 1"：那个号是**位置**的函数，不是文本的函数——写死进标题
/// 之后就得靠解析再读回来，于是每冒一个场景（中间插章、删章、捞回、中文数字、繁体）都要加一条
/// 规则。现在只给模板，号由 [`crate::numbering`] 按同层位置渲染。
fn default_title(kind: NodeKind, style: NamingStyle) -> String {
    template_for(kind, style)
}

impl Store {
    /// 在指定位置新建节点。`parent_id = None` 表示根级。
    ///
    /// **标题留空 = 按同层取号自动命名**（`第N章` 之类）——建节点的入口只有这一条，
    /// 所以"默认名怎么取"只有这一份实现，谁调都一致。
    pub fn create_node(
        &mut self,
        work_id: i64,
        parent_id: Option<i64>,
        kind: NodeKind,
        title: &str,
    ) -> Result<i64> {
        super::work::ensure_alive(&self.conn, work_id)?;
        if let Some(parent) = parent_id {
            self.ensure_node_in_work(parent, work_id)?;
        }
        let title = title.trim();
        let title = if title.is_empty() {
            // 命名规则由核心落定（作者选过 → 它；没选过 → 作品类型的默认）——只写在一处
            default_title(kind, self.naming_style(work_id)?)
        } else {
            title.to_string()
        };
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM nodes
             WHERE work_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL",
            params![work_id, parent_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO nodes(work_id, parent_id, node_kind, title, sort_order, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![work_id, parent_id, kind.as_str(), title, next, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;

        self.record(
            "nodes",
            id,
            "create",
            json!({ "work_id": work_id, "parent_id": parent_id, "kind": kind.as_str() }),
        )?;
        Ok(id)
    }

    /// 改名（标题经触发器同步进检索索引）。
    pub fn rename_node(&mut self, id: i64, title: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE nodes SET title = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![title.trim(), now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]));
        }
        self.record("nodes", id, "rename", json!({ "title": title.trim() }))
    }

    /// 写一章的"一句话"（投稿包的大纲要用它）。
    ///
    /// 存的是作者的原话，**一个字的处理都不做**（不 trim、不分句）——作者写了什么就是什么；
    /// 日志里只记字数，不把整段话抄进变更留痕。
    pub fn set_node_summary(&mut self, id: i64, summary: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE nodes SET summary = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![summary, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]));
        }
        self.record("nodes", id, "set_summary", json!({ "chars": summary.chars().count() }))
    }

    /// 移动节点到新父级的第 `index` 位（越界会夹到末尾），并把两侧同级重排成密集序号。
    pub fn move_node(&mut self, id: i64, new_parent: Option<i64>, index: usize) -> Result<()> {
        let work_id = self.node_work(id)?;
        let old_parent: Option<i64> = self
            .conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))?;

        if let Some(parent) = new_parent {
            self.ensure_node_in_work(parent, work_id)?;
            if parent == id || is_descendant(&self.conn, parent, id)? {
                return Err(Error::invalid(codes::TREE_MOVE_INTO_DESCENDANT));
            }
        }

        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE nodes SET parent_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_parent, now, id],
        )?;
        renumber(&tx, work_id, new_parent, Some((id, index)))?;
        if old_parent != new_parent {
            renumber(&tx, work_id, old_parent, None)?;
        }
        tx.commit()?;

        self.record(
            "nodes",
            id,
            "move",
            json!({ "parent_id": new_parent, "index": index }),
        )
    }

    /// 软删除节点**及其整棵子树**，返回受影响的节点数。
    pub fn soft_delete_node(&mut self, id: i64) -> Result<usize> {
        // 删之前先问清"它属于哪本书、挂在谁下面"——删完这两样就问不出来了
        let work_id = self.node_work(id)?;
        let parent_id: Option<i64> = self
            .conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?
            .flatten();

        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
             )
             UPDATE nodes SET deleted_at = ?2
             WHERE id IN (SELECT id FROM sub) AND deleted_at IS NULL",
            params![id, now_millis()],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]));
        }
        // 同级会留一个洞：**当场收成密集序号**。这样"同级序号是密集的"这条不变量
        // 在任何时候都成立，恢复时也才有一个稳定的"原来在第几位"可锚。
        renumber(&tx, work_id, parent_id, None)?;
        tx.commit()?;

        self.record("nodes", id, "delete_subtree", json!({ "affected": affected }))?;
        Ok(affected)
    }
}

impl Store {
    /// 在当前章**后面**插一章（同级、紧随其后）。
    ///
    /// 目录树接上之前，这是"再多写一章"的唯一入口；目录树的「+」将来走同一条路。
    /// 先追加到末尾再移到当前章之后——排序靠 [`Store::move_node`] 的密集序号，天然幂等。
    pub fn add_chapter_after(&mut self, node_id: i64, kind: NodeKind, title: &str) -> Result<i64> {
        let parent: Option<i64> = self
            .conn
            .query_row(
                "SELECT parent_id FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
            })?;
        let work_id = self.node_work(node_id)?;
        // **落点只有一条规矩：点哪儿插哪儿**。号是位置的函数（`{$N}` 渲染出来），
        // 所以插在中间、删掉中间、从回收站捞回，序号自己就对了——不需要任何"归位"逻辑。
        let index = sibling_ids(&self.conn, work_id, parent)?
            .iter()
            .position(|id| *id == node_id)
            .map(|position| position + 1)
            .unwrap_or(usize::MAX);

        // 标题留空 = 用模板（见 `default_title`）；作者给了名字就用他的
        let created = self.create_node(work_id, parent, kind, title)?;
        self.move_node(created, parent, index)?;
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认名是**模板**，不是写死的号：长篇的卷 / 章 / 节带计数宏；
    /// 单篇与场景卡不带号（散文、随笔、文集里作者自己起名）。
    #[test]
    fn default_names_are_templates_not_baked_numbers() {
        use crate::model::NamingStyle;
        use NamingStyle::{Arabic, Chinese, NoNumber, Padded};
        assert_eq!(default_title(NodeKind::Chapter, Arabic), "第{$N}章");
        assert_eq!(default_title(NodeKind::Chapter, Chinese), "第{$N_ZH}章");
        assert_eq!(default_title(NodeKind::Chapter, Padded), "第{$N:3}章");
        assert_eq!(default_title(NodeKind::Volume, Arabic), "第{$N}卷");
        assert_eq!(default_title(NodeKind::Section, Arabic), "第{$N}节");
        assert_eq!(default_title(NodeKind::Piece, Arabic), "第{$N}篇");
        assert_eq!(default_title(NodeKind::Chapter, NoNumber), "", "不编号：名字留给作者");
        assert_eq!(default_title(NodeKind::Piece, NoNumber), "");
        assert_eq!(default_title(NodeKind::Scene, Arabic), "", "场景卡自起名");
        // 渲染出来才是给人看的号（规则全在 numbering 套件里）
        assert_eq!(crate::numbering::render(&default_title(NodeKind::Chapter, Arabic), 3), "第3章");
        assert_eq!(crate::numbering::render(&default_title(NodeKind::Chapter, Chinese), 3), "第三章");
        assert_eq!(crate::numbering::render(&default_title(NodeKind::Chapter, Padded), 3), "第003章");
    }
}
