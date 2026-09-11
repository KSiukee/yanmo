//! 节点树的**读**：目录树、单层懒加载、归属检查。
//!
//! - 只返回 [`NodeSummary`]（元数据，**没有正文字段**）——懒加载靠类型保证，不靠自觉；
//! - "卷 / 章 / 节 / 单篇 / 场景卡"只是 `node_kind` 的取值，**代码里不得假设层级**；
//! - 编辑（建 / 改名 / 移动 / 删除）在 [`super::node_edit`]，读写分家免得互相拖累。

use rusqlite::{params, OptionalExtension};

use super::{Store, MAX_TREE_DEPTH};
use crate::error::{Error, Result};
use crate::model::NodeKind;

/// 目录树条目：**没有正文字段**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSummary {
    pub id: i64,
    pub work_id: i64,
    pub parent_id: Option<i64>,
    pub kind: NodeKind,
    pub title: String,
    pub sort_order: i64,
    /// 预聚合字数（来自 `nodes.word_count`，不扫正文）
    pub word_count: i64,
    /// 是否已有正文——空章一眼可见；但**正文本身不在这里**
    pub has_body: bool,
    /// 下面还有没有节点（界面据此决定要不要画展开箭头）。
    ///
    /// 只问"有没有"，**不问有几个、更不问是什么**——展开箭头不该顺带把整棵子树拖出来。
    pub has_children: bool,
}

/// 树条目查询的公共部分：只取元数据 + "有没有正文 / 有没有下级" 的判断，**不取正文**。
const SUMMARY_SQL: &str = "SELECT n.id, n.work_id, n.parent_id, n.node_kind, n.title,
        n.sort_order, n.word_count,
        (c.body IS NOT NULL AND c.body <> '') AS has_body,
        EXISTS(SELECT 1 FROM nodes k WHERE k.parent_id = n.id AND k.deleted_at IS NULL) AS has_children
     FROM nodes n
     JOIN works w ON w.id = n.work_id AND w.deleted_at IS NULL
     LEFT JOIN node_contents c ON c.node_id = n.id
     WHERE n.deleted_at IS NULL";

type SummaryRow = (i64, i64, Option<i64>, String, String, i64, i64, i64, i64);

fn read_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SummaryRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn build_summary(row: SummaryRow) -> Result<NodeSummary> {
    Ok(NodeSummary {
        id: row.0,
        work_id: row.1,
        parent_id: row.2,
        kind: NodeKind::parse(&row.3)?,
        title: row.4,
        sort_order: row.5,
        word_count: row.6,
        has_body: row.7 != 0,
        has_children: row.8 != 0,
    })
}

impl Store {
    /// 整棵树的元数据（目录小的时候一次拉完）。
    ///
    /// 返回的是**扁平列表**，按（父节点, 顺序）排列，调用方按 `parent_id` 自行组装树。
    pub fn list_nodes(&self, work_id: i64) -> Result<Vec<NodeSummary>> {
        let sql =
            format!("{SUMMARY_SQL} AND n.work_id = ?1 ORDER BY n.parent_id, n.sort_order, n.id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id], read_summary)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(build_summary(row?)?);
        }
        Ok(out)
    }

    /// 只取某一层的子节点——**展开哪一层拉哪一层**（长篇目录的懒加载入口）。
    pub fn children_of(&self, work_id: i64, parent_id: Option<i64>) -> Result<Vec<NodeSummary>> {
        let sql = format!(
            "{SUMMARY_SQL} AND n.work_id = ?1 AND n.parent_id IS ?2 ORDER BY n.sort_order, n.id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![work_id, parent_id], read_summary)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(build_summary(row?)?);
        }
        Ok(out)
    }

    /// 节点标题（导出文件名、界面提示要用；顺带确认节点活着）。
    pub fn node_title(&self, node_id: i64) -> Result<String> {
        self.conn
            .query_row(
                "SELECT title FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Invalid(format!("节点不存在或已删除：{node_id}")))
    }

    /// 节点所属作品（顺带确认它存在且未删除）。
    pub(super) fn node_work(&self, id: i64) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT work_id FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Invalid(format!("节点不存在或已删除：{id}")))
    }

    /// 确认节点属于指定作品——**防跨作品挂错父级**。
    pub(super) fn ensure_node_in_work(&self, node_id: i64, work_id: i64) -> Result<()> {
        let owner = self.node_work(node_id)?;
        if owner != work_id {
            return Err(Error::Invalid(format!(
                "节点 {node_id} 属于作品 {owner}，不能挂到作品 {work_id} 下"
            )));
        }
        Ok(())
    }

    /// 从**根到该节点父级**的 id 链（不含它自己）——"打开就定位到正在写的那一章"要用。
    ///
    /// 逐级往上走，不写递归查询：链长就是树的深度（通常个位数），
    /// 而且深度上限一眼看得见——数据坏了会明确报错，不会转到天荒地老。
    pub fn node_ancestors(&self, node_id: i64) -> Result<Vec<i64>> {
        self.node_work(node_id)?; // 顺带确认它存在且没被删
        let parent_of = |id: i64| -> Result<Option<i64>> {
            Ok(self
                .conn
                .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![id], |r| r.get(0))?)
        };
        let mut chain = Vec::new();
        let mut current = parent_of(node_id)?;
        while let Some(id) = current {
            if chain.len() >= MAX_TREE_DEPTH {
                return Err(Error::Invalid(
                    "节点树深度异常（疑似成环），已拒绝继续".to_string(),
                ));
            }
            chain.push(id);
            current = parent_of(id)?;
        }
        chain.reverse(); // 根在前，界面照着一层层展开就行
        Ok(chain)
    }
}

/// 导航用的章节条目（完整目录树是另一个任务；这里只给"上一章 / 下一章"够用的东西）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterSummary {
    pub id: i64,
    pub title: String,
    pub word_count: i64,
}

/// 当前章的邻居与位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterNeighbors {
    pub previous: Option<ChapterSummary>,
    pub next: Option<ChapterSummary>,
    /// 从 1 开始的序号（给人看的）
    pub index: i64,
    pub total: i64,
}

impl Store {
    /// 上一章 / 下一章——按**阅读顺序**（深度优先，跨卷），不是简单的同级前后。
    ///
    /// 长篇里"下一章"经常要跨过卷的边界；只按同级排会在这里断掉。
    pub fn chapter_neighbors(&self, node_id: i64) -> Result<ChapterNeighbors> {
        let work_id = self.node_work(node_id)?;
        let order = self.reading_order(work_id)?;
        let index = order
            .iter()
            .position(|chapter| chapter.id == node_id)
            .ok_or_else(|| Error::Invalid(format!("节点不承载正文，不能当章节导航：{node_id}")))?;
        Ok(ChapterNeighbors {
            previous: index.checked_sub(1).and_then(|i| order.get(i).cloned()),
            next: order.get(index + 1).cloned(),
            index: index as i64 + 1,
            total: order.len() as i64,
        })
    }

    /// 全书的阅读顺序：按父节点分组后深度优先展开，只保留承载正文的节点。
    fn reading_order(&self, work_id: i64) -> Result<Vec<ChapterSummary>> {
        let nodes = self.list_nodes(work_id)?; // 已按（父节点, 顺序）排好
        let mut children: std::collections::HashMap<Option<i64>, Vec<usize>> =
            std::collections::HashMap::new();
        for (position, node) in nodes.iter().enumerate() {
            children.entry(node.parent_id).or_default().push(position);
        }

        let mut order = Vec::new();
        let mut stack: Vec<usize> = children
            .get(&None)
            .map(|roots| roots.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(position) = stack.pop() {
            let node = &nodes[position];
            if node.kind.holds_body() {
                order.push(ChapterSummary {
                    id: node.id,
                    title: node.title.clone(),
                    word_count: node.word_count,
                });
            }
            if let Some(kids) = children.get(&Some(node.id)) {
                for &kid in kids.iter().rev() {
                    stack.push(kid);
                }
            }
        }
        Ok(order)
    }
}
