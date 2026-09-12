//! 回收站：软删除项的**列出 / 恢复 / 彻底删除**。
//!
//! 四条纪律：
//!
//! 1. **恢复是"整棵子树 + 必要的父链"一起回来**——只把节点本身的标记擦掉，
//!    它却挂在一个还在回收站里的父级下面，界面上根本看不见，等于没恢复；
//! 2. **彻底删除只对"已经在回收站里"的东西开放**——不给"绕过回收站直接抹掉"留后门；
//! 3. 真删是数据库级的 `DELETE`，正文与快照跟着走（外键级联），**这一步之后没有后悔药**；
//! 4. 删掉一本书只是给作品行打时间戳，**书里单独删过的章仍然留在回收站里**
//!    （那是另一次删除，不该被"恢复整本书"顺手撤销）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{Store, MAX_TREE_DEPTH};
use crate::error::{codes, Error, Result};

/// 回收站里的东西是"整本书"还是"书里的某一段"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashKind {
    Work,
    Node,
}

impl TrashKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TrashKind::Work => "work",
            TrashKind::Node => "node",
        }
    }
}

/// 同级里与它重名、还活着的那个（恢复前要摆给作者看）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameClash {
    pub id: i64,
    pub title: String,
    /// 它现在有多少字（让作者知道"字还在"）——按词口径
    pub word_count: i64,
    /// 同上，逐字（含标点）
    pub char_count: i64,
    /// 同上，逐字（不含标点）
    pub chars_no_punct: i64,
}

/// 恢复**之前**先看一眼：它会回到哪、会不会与谁重名。
///
/// 恢复是一次点击就能完成的动作，而"删了旧的、又重写了同名的一章"这种事一模一样地
/// 长得像误触——所以**把冲突摆出来让作者选**，系统绝不替他决定，也绝不替他改名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePreview {
    pub work_id: i64,
    pub work_title: String,
    /// 原来的父级；`None` = 根级
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    /// 原来在第几位（从 1 起）
    pub index: i64,
    /// 同级里同名的活节点（空的 = 没有冲突，直接恢复就好）
    pub name_clashes: Vec<NameClash>,
}

/// 回收站里的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    pub kind: TrashKind,
    pub id: i64,
    pub title: String,
    /// 属于哪本书（书自己在回收站里时，就是它自己）
    pub work_id: i64,
    pub work_title: String,
    pub deleted_at: i64,
    /// 跟着一起进来的节点数（含它自己）——"删了一卷"要让人看出带走了多少
    pub nodes: i64,
}

impl Store {
    /// 回收站里的东西：整本书 + 书里被删的那些段。
    ///
    /// 只列**最上面那一层**被删的节点：一卷和它里面的章是一起删的，
    /// 列两个等于让同一件事出现两次。
    pub fn list_trash(&self) -> Result<Vec<TrashEntry>> {
        let mut out = Vec::new();

        // ① 整本书
        let mut stmt = self.conn.prepare(
            "SELECT w.id, w.title, w.deleted_at,
                    (SELECT COUNT(*) FROM nodes n
                      WHERE n.work_id = w.id AND n.deleted_at IS NULL)
               FROM works w
              WHERE w.deleted_at IS NOT NULL
              ORDER BY w.deleted_at DESC, w.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (id, title, deleted_at, nodes) = row?;
            out.push(TrashEntry {
                kind: TrashKind::Work,
                id,
                work_id: id,
                work_title: title.clone(),
                title,
                deleted_at,
                nodes,
            });
        }

        // ② 还活着的书里、被删掉的那些段（父级没被删的才算"最上面那一层"）
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.title, n.deleted_at, w.id, w.title
               FROM nodes n
               JOIN works w ON w.id = n.work_id AND w.deleted_at IS NULL
              WHERE n.deleted_at IS NOT NULL
                AND (n.parent_id IS NULL
                     OR (SELECT p.deleted_at FROM nodes p WHERE p.id = n.parent_id) IS NULL)
              ORDER BY n.deleted_at DESC, n.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut segments = Vec::new();
        for row in rows {
            let (id, title, deleted_at, work_id, work_title) = row?;
            let nodes = self.subtree_size(id)?;
            segments.push(TrashEntry {
                kind: TrashKind::Node,
                id,
                title,
                work_id,
                work_title,
                deleted_at,
                nodes,
            });
        }
        out.extend(segments); // 整本书在前（它们是更大的那一坨），段落在后
        Ok(out)
    }

    /// 恢复一本书。
    ///
    /// 只擦作品行上的时间戳：书里那些**单独被删过的章**留在回收站里不动——
    /// 那是另一次删除，"恢复整本书"不该顺手把它撤销。
    pub fn restore_work(&mut self, work_id: i64) -> Result<usize> {
        if !self.trashed("works", work_id)? {
            return Err(Error::invalid_with(codes::WORK_NOT_TRASHED, [("work_id", work_id.to_string())]));
        }
        self.conn.execute(
            "UPDATE works SET deleted_at = NULL WHERE id = ?1",
            params![work_id],
        )?;
        self.record("works", work_id, "restore", json!({}))?;
        Ok(1)
    }

    /// 恢复一个节点：**整棵子树 + 还在回收站里的父链**一起回来。
    ///
    /// `rename_to` 是作者在"有重名"时给的新名字——**名字永远由他给**，系统不替他起；
    /// 改名与恢复在同一个事务里，所以不会出现"两章同名"的中间状态。
    ///
    /// 返回恢复的节点数（含父链）。
    pub fn restore_node(&mut self, node_id: i64, rename_to: Option<&str>) -> Result<usize> {
        if !self.trashed("nodes", node_id)? {
            return Err(Error::invalid_with(codes::NODE_NOT_TRASHED, [("node_id", node_id.to_string())]));
        }
        let work_id = self.work_of_any(node_id)?;
        // 记下每一层"原来在第几位"当锚：被删那天它停在哪，恢复就回到那一带
        let mut anchors: Vec<(i64, Option<i64>, i64)> = Vec::new();
        let mut current = Some(node_id);
        while let Some(id) = current {
            if anchors.len() > MAX_TREE_DEPTH {
                return Err(Error::invalid(codes::TREE_CYCLE_SUSPECTED));
            }
            let (parent, order, deleted): (Option<i64>, i64, Option<i64>) = self
                .conn
                .query_row(
                    "SELECT parent_id, sort_order, deleted_at FROM nodes WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
                .ok_or_else(|| Error::invalid_with(codes::NODE_NOT_FOUND, [("node_id", id.to_string())]))?;
            anchors.push((id, parent, order));
            if deleted.is_none() {
                break; // 这条链往上已经都是活的了
            }
            current = parent;
        }

        let tx = self.conn.transaction()?;
        let restored = tx.execute(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
             )
             UPDATE nodes SET deleted_at = NULL WHERE id IN (SELECT id FROM sub)",
            params![node_id],
        )?;
        // 父链：捞出来的东西不能挂在看不见的父级下面
        for (id, _, _) in anchors.iter().skip(1) {
            tx.execute("UPDATE nodes SET deleted_at = NULL WHERE id = ?1", params![id])?;
        }
        if let Some(title) = rename_to.map(str::trim).filter(|title| !title.is_empty()) {
            tx.execute(
                "UPDATE nodes SET title = ?1 WHERE id = ?2",
                params![title, node_id],
            )?;
        }
        // 每一层都按"原来的位置"锚回去，再收成密集序号——回到原来那一带，且不与谁同号
        for (id, parent, order) in &anchors {
            super::node_edit::renumber(&tx, work_id, *parent, Some((*id, *order as usize)))?;
        }
        tx.commit()?;

        self.record(
            "nodes",
            node_id,
            "restore",
            json!({ "with_parents": true, "renamed": rename_to.is_some() }),
        )?;
        Ok(restored)
    }

    /// 恢复**之前**先看一眼：回到哪、会不会与同级某章重名。
    pub fn restore_preview(&self, node_id: i64) -> Result<RestorePreview> {
        if !self.trashed("nodes", node_id)? {
            return Err(Error::invalid_with(codes::NODE_NOT_TRASHED, [("node_id", node_id.to_string())]));
        }
        let work_id = self.work_of_any(node_id)?;
        let work_title: String = self
            .conn
            .query_row("SELECT title FROM works WHERE id = ?1", params![work_id], |r| r.get(0))?;
        let (parent_id, title, order): (Option<i64>, String, i64) = self.conn.query_row(
            "SELECT parent_id, title, sort_order FROM nodes WHERE id = ?1",
            params![node_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let parent_title: Option<String> = match parent_id {
            Some(id) => Some(
                self.conn
                    .query_row("SELECT title FROM nodes WHERE id = ?1", params![id], |r| r.get(0))
                    .optional()?
                    .unwrap_or_default(),
            ),
            None => None,
        };

        let mut stmt = self.conn.prepare(
            "SELECT id, title, word_count, char_count, chars_no_punct FROM nodes
              WHERE work_id = ?1 AND parent_id IS ?2 AND deleted_at IS NULL AND title = ?3
              ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map(params![work_id, parent_id, title], |r| {
            Ok(NameClash {
                id: r.get(0)?,
                title: r.get(1)?,
                word_count: r.get(2)?,
                char_count: r.get(3)?,
                chars_no_punct: r.get(4)?,
            })
        })?;
        let mut name_clashes = Vec::new();
        for row in rows {
            name_clashes.push(row?);
        }

        Ok(RestorePreview {
            work_id,
            work_title,
            parent_id,
            parent_title,
            index: order + 1,
            name_clashes,
        })
    }

    /// 彻底删除一个节点（**不可恢复**）：连同子树、正文与快照一起抹掉。
    ///
    /// 返回删掉的节点数。
    pub fn purge_node(&mut self, node_id: i64) -> Result<usize> {
        if !self.trashed("nodes", node_id)? {
            return Err(Error::invalid_with(
                codes::TRASH_PURGE_NEEDS_TRASHED,
                [("id", node_id.to_string())],
            ));
        }
        let removed = self.conn.execute(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
             )
             DELETE FROM nodes WHERE id IN (SELECT id FROM sub)",
            params![node_id],
        )?;
        self.record("nodes", node_id, "purge", json!({ "removed": removed }))?;
        Ok(removed)
    }

    /// 彻底删除一本书（**不可恢复**）：书里所有节点、正文与快照一起走。
    pub fn purge_work(&mut self, work_id: i64) -> Result<usize> {
        if !self.trashed("works", work_id)? {
            return Err(Error::invalid_with(
                codes::TRASH_PURGE_NEEDS_TRASHED,
                [("id", work_id.to_string())],
            ));
        }
        let nodes: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE work_id = ?1",
            params![work_id],
            |r| r.get(0),
        )?;
        // 节点、正文、快照由外键级联带走（连接打开了 foreign_keys）
        self.conn
            .execute("DELETE FROM works WHERE id = ?1", params![work_id])?;
        self.record("works", work_id, "purge", json!({ "nodes": nodes }))?;
        Ok(nodes as usize)
    }

    /// 清空回收站：**所有**软删的东西一起走。返回清掉的项数。
    pub fn empty_trash(&mut self) -> Result<usize> {
        let entries = self.list_trash()?;
        for entry in entries.iter().filter(|e| e.kind == TrashKind::Node) {
            self.purge_node(entry.id)?;
        }
        for entry in entries.iter().filter(|e| e.kind == TrashKind::Work) {
            self.purge_work(entry.id)?;
        }
        Ok(entries.len())
    }

    /// 这一行是不是躺在回收站里。
    fn trashed(&self, table: &str, id: i64) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                &format!("SELECT id FROM {table} WHERE id = ?1 AND deleted_at IS NOT NULL"),
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// 这个节点属于哪本书（**连已删除的也算**）：删着的东西也要能问出归属。
    pub(super) fn work_of_any(&self, node_id: i64) -> Result<i64> {
        self.conn
            .query_row("SELECT work_id FROM nodes WHERE id = ?1", params![node_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::invalid_with(codes::NODE_NOT_FOUND, [("node_id", node_id.to_string())]))
    }

    /// 子树里有几个节点（含自己）。
    fn subtree_size(&self, node_id: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id FROM nodes n JOIN sub ON n.parent_id = sub.id
             )
             SELECT COUNT(*) FROM sub",
            params![node_id],
            |r| r.get(0),
        )?)
    }
}
