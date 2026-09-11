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

use super::Store;
use crate::error::{Error, Result};

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
            return Err(Error::Invalid(format!("回收站里没有这本书：{work_id}")));
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
    /// 返回恢复的节点数（含父链）。
    pub fn restore_node(&mut self, node_id: i64) -> Result<usize> {
        if !self.trashed("nodes", node_id)? {
            return Err(Error::Invalid(format!("回收站里没有这一段：{node_id}")));
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
        let mut current: Option<i64> = tx
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![node_id], |r| r.get(0))
            .optional()?
            .flatten();
        while let Some(id) = current {
            let still_deleted: Option<i64> = tx
                .query_row(
                    "SELECT id FROM nodes WHERE id = ?1 AND deleted_at IS NOT NULL",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            if still_deleted.is_none() {
                break; // 这条链往上已经都是活的了
            }
            tx.execute("UPDATE nodes SET deleted_at = NULL WHERE id = ?1", params![id])?;
            current = tx
                .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))
                .optional()?
                .flatten();
        }
        tx.commit()?;

        self.record("nodes", node_id, "restore", json!({ "with_parents": true }))?;
        Ok(restored)
    }

    /// 彻底删除一个节点（**不可恢复**）：连同子树、正文与快照一起抹掉。
    ///
    /// 返回删掉的节点数。
    pub fn purge_node(&mut self, node_id: i64) -> Result<usize> {
        if !self.trashed("nodes", node_id)? {
            return Err(Error::Invalid(format!(
                "只能彻底删除已经在回收站里的东西：{node_id}"
            )));
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
            return Err(Error::Invalid(format!(
                "只能彻底删除已经在回收站里的东西：{work_id}"
            )));
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
