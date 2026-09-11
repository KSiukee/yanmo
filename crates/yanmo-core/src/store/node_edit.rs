//! 节点树的**编辑**：建 / 改名 / 移动 / 软删除。
//!
//! - 同级排序是**密集序号**（0,1,2…）：每次移动后重排，幂等、无空洞、结果可复现；
//! - 移动前做**成环检测**：不允许把节点移进自己的子孙——树坏掉比报错难查得多；
//! - 软删除连带整棵子树，正文与历史都留着（回收站与"删错了"的撤销靠它）；
//! - 读在 [`super::node`]，两边分开，改编辑不会牵动目录树查询。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::{Store, MAX_TREE_DEPTH};
use crate::error::{Error, Result};
use crate::model::NodeKind;
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
    Err(Error::Invalid(
        "节点树深度异常（疑似成环），已拒绝继续".to_string(),
    ))
}

/// 默认名的"前缀 / 后缀"：`第 12 章` 这种编号的骨架（场景卡没有"第…章"的说法，给个朴素名字）。
fn naming(kind: NodeKind) -> (&'static str, &'static str) {
    match kind {
        NodeKind::Volume => ("第", "卷"),
        NodeKind::Chapter => ("第", "章"),
        NodeKind::Section => ("第", "节"),
        NodeKind::Piece => ("第", "篇"),
        NodeKind::Scene => ("场景卡", ""),
    }
}

/// 从标题里认出编号：阿拉伯数字与中文数字都认（作者手打的「第一章」也算数），
/// 其它名字一概不猜，也就不会误判成编号。
fn parse_serial(title: &str, kind: NodeKind) -> Option<i64> {
    let (prefix, suffix) = naming(kind);
    let inner = title.strip_prefix(prefix)?.strip_suffix(suffix)?.trim();
    inner
        .parse::<i64>()
        .ok()
        .or_else(|| parse_cn_number(inner))
        .filter(|serial| *serial > 0)
}

/// 认中文数字：一 / 十 / 十二 / 二十三 …（到九十九够用了）。
fn parse_cn_number(text: &str) -> Option<i64> {
    let digit = |c: char| match c {
        '零' => Some(0),
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    };
    let chars: Vec<char> = text.chars().collect();
    match chars.as_slice() {
        // ⚠️ '十' 必须排在单字那一支前面：match 是自上而下的，
        // 先命中 [single] 的话「第十章」会被当成"一个认不出的字"（表驱动测试抓到的）
        ['十'] => Some(10),
        [single] => digit(*single),
        ['十', b] => digit(*b).map(|b| 10 + b),
        [a, '十'] => digit(*a).map(|a| a * 10),
        [a, '十', b] => digit(*a).and_then(|a| digit(*b).map(|b| a * 10 + b)),
        _ => None,
    }
}

/// 标题留空时的默认名——**按同层同类型取号**（卷 / 章 / 节 / 篇），不按全书。
///
/// 按全书数会把别卷的章也算进来，分卷之后新建的章就会跳号；按这一层取号，
/// 才是作者在这一卷里看到的"下一章"。
///
/// 规则只有一句话：**紧挨着的两个号中间正好缺一个，就补那个缺号；其余一律取最大号 + 1。**
///
/// - 删掉第 2 章之后，在第 1 章后面插一章（左右是 1 和 3，中间正好缺 2）→ 就叫「第2章」✅
/// - 在这本书末尾接着写（右边没有号了）→ 「最大号 + 1」✅
/// - 层里的编号是 5、6 这种（插在中间也算不出"缺的那个号"）→ 一律「最大号 + 1」，
///   绝不凭空给一个跟这一段编号无关的号（早先的"按第几位取号"就会在那儿算出「第2章」）
/// - 一个编号都没认出来（作者全用自起的名字）→ 退回"同层同类现有几章 + 1"
fn default_title(
    conn: &Connection,
    work_id: i64,
    parent_id: Option<i64>,
    kind: NodeKind,
    after: Option<i64>,
) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT id, title FROM nodes
         WHERE work_id = ?1 AND parent_id IS ?2 AND node_kind = ?3 AND deleted_at IS NULL
         ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![work_id, parent_id, kind.as_str()], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut siblings: Vec<(i64, Option<i64>)> = Vec::new();
    for row in rows {
        let (id, title) = row?;
        siblings.push((id, parse_serial(&title, kind)));
    }

    let max_used = siblings.iter().filter_map(|(_, serial)| *serial).max();
    let (prefix, suffix) = naming(kind);
    let serial = fill_single_gap(&siblings, after)
        .or_else(|| max_used.map(|max| max + 1))
        .unwrap_or(siblings.len() as i64 + 1);
    Ok(format!("{prefix}{serial}{suffix}"))
}

/// 落点左右**紧挨着**的两个号如果正好差 2，就补中间那个缺号——除此之外不猜。
///
/// 左边没有号（落点不是同类兄弟，或它自己没编号）就当作 0（"排在这一层最前面"），
/// 右边没有号（插在末尾）就返回 `None`，交给"最大号 + 1"。
fn fill_single_gap(siblings: &[(i64, Option<i64>)], after: Option<i64>) -> Option<i64> {
    let anchor = after?;
    let position = siblings.iter().position(|(id, _)| *id == anchor);
    let left = position.and_then(|index| siblings[index].1).unwrap_or(0);
    let start = position.map(|index| index + 1).unwrap_or(0);
    let right = siblings.get(start..)?.iter().find_map(|(_, serial)| *serial)?;
    (right - left == 2).then_some(left + 1)
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
            default_title(&self.conn, work_id, parent_id, kind, None)?
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
            return Err(Error::Invalid(format!("节点不存在或已删除：{id}")));
        }
        self.record("nodes", id, "rename", json!({ "title": title.trim() }))
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
                return Err(Error::Invalid(
                    "不能把节点移进自己的子孙里——那会形成环".to_string(),
                ));
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
            return Err(Error::Invalid(format!("节点不存在或已删除：{id}")));
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
            .ok_or_else(|| Error::Invalid(format!("节点不存在或已删除：{node_id}")))?;
        let work_id = self.node_work(node_id)?;
        let index = sibling_ids(&self.conn, work_id, parent)?
            .iter()
            .position(|id| *id == node_id)
            .map(|position| position + 1)
            .unwrap_or(usize::MAX);

        // 标题留空时**按落点取号**：作者在第 1 章后面插一章，它就该叫「第2章」
        let title = if title.trim().is_empty() {
            default_title(&self.conn, work_id, parent, kind, Some(node_id))?
        } else {
            title.trim().to_string()
        };
        let created = self.create_node(work_id, parent, kind, &title)?;
        self.move_node(created, parent, index)?;
        Ok(created)
    }
}
