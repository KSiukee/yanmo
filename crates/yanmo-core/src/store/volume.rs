//! 分卷：**在这里收卷 / 撤卷**——只动结构，一个字都不碰正文。
//!
//! # 两种收卷形态（按收卷点所在的那一层自动判断，作者不必先选模式）
//!
//! - **卷内**（收卷点在某一卷里）：这一卷到此为止——在它后面新建下一卷，
//!   把收卷点之后的章一并挪进新卷；若后面一章都还没写，就在新卷里起第一章，
//!   光标落过去接着写（下一章在新卷里重新起号）；
//! - **根层**（收卷点还散在根上、没进过任何卷）：把「上一个卷之后、到这一点为止」
//!   这一段散章收成一个新卷，光标不动（他正写的那一章还在原地，只是进了卷）。
//!
//! 撤卷是这两种形态的**逆操作**：把这一卷的孩子按原顺序并回**它前面那个卷**；
//! 前面没有卷，就抬到父层、占据这一卷原来的位置——两种成卷都正好还原。
//! 然后这一卷**软删**（进回收站，捞得回来），所以"一键撤销"反悔两次也不会丢东西。
//!
//! # 三条纪律
//!
//! - **只动结构、不动正文**：全程不碰 `node_contents`（有断言钉着）；
//! - 排序与序号交给既有的 [`Store::create_node`] / [`Store::move_node`]——
//!   密集序号、幂等，这里不自己写排序 SQL；
//! - **阈值只向 [`crate::volume`] 要**：这一层只管"库里现在什么样"。

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::{node_edit, NodeSummary, Store};
use crate::error::{codes, Error, Result};
use crate::model::NodeKind;
use crate::volume::{self, VolumePlan, VolumeSpot};

/// 一次收卷的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseReceipt {
    /// 新建的那一卷
    pub volume_id: i64,
    /// 新卷是空的，顺手在里面起的**第一章**（界面把光标落过去）；有章可搬时是 `None`
    pub opened_chapter: Option<i64>,
    /// 挪进新卷的节点数（章 / 场景卡都算——整段都属于新卷）
    pub moved: usize,
}

/// 一次撤卷的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DissolveReceipt {
    /// 从这一卷里抬出去、还回原位的节点数
    pub moved: usize,
    /// 并进了哪一卷（`None` = 抬到父层原位）
    pub merged_into: Option<i64>,
}

/// 把一棵树摊平：只够回答两件事——"某卷里有几章""收卷点是这一卷的第几章"。
struct TreeView {
    nodes: Vec<NodeSummary>,
    at: HashMap<i64, usize>,
    children: HashMap<Option<i64>, Vec<usize>>,
}

impl TreeView {
    fn build(nodes: Vec<NodeSummary>) -> Self {
        let mut at = HashMap::with_capacity(nodes.len());
        let mut children: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
        for (position, node) in nodes.iter().enumerate() {
            at.insert(node.id, position);
            children.entry(node.parent_id).or_default().push(position);
        }
        Self { nodes, at, children }
    }

    fn node(&self, id: i64) -> Option<&NodeSummary> {
        self.at.get(&id).map(|position| &self.nodes[*position])
    }

    /// 根层的孩子，按库里给的顺序。
    fn roots(&self) -> Vec<&NodeSummary> {
        self.children
            .get(&None)
            .map(|list| list.iter().map(|position| &self.nodes[*position]).collect())
            .unwrap_or_default()
    }

    /// 从 `root`（`None` = 根层）往下**按阅读顺序**（深度优先、父在子前）挑出某一类节点。
    ///
    /// 预算是节点总数：一棵树每个节点最多走一次，超了就说明数据成环——
    /// **停手就好**，不能转到天荒地老（与读路径的深度上限同一条纪律）。
    fn walk(&self, root: Option<i64>, kind: NodeKind) -> Vec<i64> {
        let mut out = Vec::new();
        let mut budget = self.nodes.len() + 1;
        let mut stack: Vec<usize> = self
            .children
            .get(&root)
            .map(|list| list.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(position) = stack.pop() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let node = &self.nodes[position];
            if node.kind == kind {
                out.push(node.id);
            }
            if let Some(kids) = self.children.get(&Some(node.id)) {
                for &kid in kids.iter().rev() {
                    stack.push(kid);
                }
            }
        }
        out
    }

    /// 收卷点"是这一卷的第几章" + 它落在哪一卷上。
    ///
    /// - 卷内：这一卷里到它为止的章数（跟目录里「本卷 12/30 章」是同一个数）；
    /// - 根层：**上一个卷之后**这一段散章里到它为止的章数，卷是 `None`；
    /// - 其余（节、场景卡、挂在章下面的收卷点）：收不了卷，`None`。
    fn spot_of(&self, node_id: i64) -> Option<(Option<i64>, i64)> {
        let node = self.node(node_id)?;
        if node.kind != NodeKind::Chapter {
            return None;
        }
        match node.parent_id {
            Some(parent) if self.node(parent)?.kind == NodeKind::Volume => {
                // **已经收好的卷不再问**：这一卷后面还有卷，说明它的边界早定了——
                // 在这里再收一次会在两卷之间塞进一个新卷，把后面的章整体挤到更后面
                // （真机上就是"点了一下，凭空多出一卷、后一卷的章还跑到新卷后面去了"）
                if self.walk(None, NodeKind::Volume).last().copied() != Some(parent) {
                    return None;
                }
                self.walk(Some(parent), NodeKind::Chapter)
                    .iter()
                    .position(|id| *id == node_id)
                    .map(|at| (Some(parent), at as i64 + 1))
            }
            None => {
                let roots = self.roots();
                let at = roots.iter().position(|node| node.id == node_id)?;
                let start = roots
                    .iter()
                    .rposition(|node| node.kind == NodeKind::Volume)
                    .map(|last| last + 1)
                    .unwrap_or(0);
                if at < start {
                    return None; // 落在已收好的卷那一段里：收卷点在这儿没有意义
                }
                let count = roots[start..=at]
                    .iter()
                    .filter(|node| node.kind == NodeKind::Chapter)
                    .count() as i64;
                Some((None, count))
            }
            _ => None,
        }
    }
}

impl Store {
    /// 这本书的分卷口径（界面显示「本卷 12/30 章」的分母就是 `effective`）。
    pub fn volume_plan(&self, work_id: i64) -> Result<VolumePlan> {
        let view = TreeView::build(self.list_nodes(work_id)?);
        self.plan_of(work_id, &view)
    }

    /// 该不该在**这一章之后**提一句收卷（`None` = 不用提）。
    pub fn volume_offer(&self, node_id: i64) -> Result<Option<VolumeSpot>> {
        let work_id = self.node_work(node_id)?;
        let view = TreeView::build(self.list_nodes(work_id)?);
        let Some((container, count)) = view.spot_of(node_id) else {
            return Ok(None);
        };
        let plan = self.plan_of(work_id, &view)?;
        Ok(volume::offer(plan, count).map(|offer| VolumeSpot { container, offer }))
    }

    /// 口径：历史 = 除**最后一卷**（正在写的这一卷）以外，每一卷收在几章。
    fn plan_of(&self, work_id: i64, view: &TreeView) -> Result<VolumePlan> {
        let volumes = view.walk(None, NodeKind::Volume);
        let finished = &volumes[..volumes.len().saturating_sub(1)];
        let history: Vec<i64> =
            finished.iter().map(|id| view.walk(Some(*id), NodeKind::Chapter).len() as i64).collect();
        Ok(volume::effective_target(self.volume_target(work_id)?, &history))
    }

    /// 读一个节点的（父，类型）——收卷要先看清"收卷点落在哪一层"。
    fn boundary_of(&self, node_id: i64) -> Result<(Option<i64>, NodeKind)> {
        let row: Option<(Option<i64>, String)> = self
            .conn
            .query_row(
                "SELECT parent_id, node_kind FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (parent, kind) = row.ok_or_else(|| {
            Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
        })?;
        Ok((parent, NodeKind::parse(&kind)?))
    }

    /// **在这里收卷**：新建一卷、把该进去的东西挪进去（形态见模块头）。
    ///
    /// `title` 留空 = 按位置渲染成「第 N 卷」（作者可以回头在树上就地改名）。
    pub fn close_volume(&mut self, node_id: i64, title: &str) -> Result<CloseReceipt> {
        let work_id = self.node_work(node_id)?;
        let (parent, kind) = self.boundary_of(node_id)?;
        if kind != NodeKind::Chapter {
            return Err(Error::invalid(codes::VOLUME_CLOSE_POINT));
        }
        let Some(container) = parent else {
            return self.close_at_root(work_id, node_id, title);
        };
        let (up, container_kind) = self.boundary_of(container)?;
        if container_kind != NodeKind::Volume {
            return Err(Error::invalid(codes::VOLUME_CLOSE_POINT));
        }

        // 收卷点之后、还在这一卷里的那些（含场景卡这类整段带走的东西）
        let kids = self.children_of(work_id, Some(container))?;
        let at = kids.iter().position(|node| node.id == node_id).ok_or_else(|| {
            Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
        })?;
        let tail: Vec<i64> = kids[at + 1..].iter().map(|node| node.id).collect();
        let siblings = self.children_of(work_id, up)?;
        let volume_at = siblings.iter().position(|node| node.id == container).unwrap_or(0);
        let naming = self.naming_style(work_id)?;

        // —— 读完了，从这里开始只写：**整段收卷在一个事务里**，中途任何一步失败都不会留下
        // "新卷建好了、一部分章还在外面"的半成品（2026-09-15 代码质量评审：严重 4）。
        // 先读后写是安全的：同一个库同时只允许一个研墨在写（单实例锁）。
        let tx = self.conn.transaction()?;
        // 新卷紧跟在当前卷后面（兄弟位置）——号按位置渲染，这里只管顺序
        let new_volume =
            node_edit::create_node_in(&tx, work_id, up, NodeKind::Volume, title, naming)?;
        node_edit::move_node_in(&tx, new_volume, up, volume_at + 1)?;
        for chapter in &tail {
            node_edit::move_node_in(&tx, *chapter, Some(new_volume), usize::MAX)?;
        }

        // 空卷写不了字：顺手起第一章，光标才落得过去（新卷从「第1章」重新起号）
        let opened = if tail.is_empty() {
            Some(node_edit::create_node_in(
                &tx,
                work_id,
                Some(new_volume),
                NodeKind::Chapter,
                "",
                naming,
            )?)
        } else {
            None
        };
        let moved = tail.len();
        // 留痕也在这个事务里（评审：中等 6）：报失败必须意味着"整段卷真的没收"
        Self::record_in(
            &self.device_id,
            &tx,
            "nodes",
            new_volume,
            "close_volume",
            json!({ "after": node_id, "moved": moved, "opened": opened }),
        )?;
        tx.commit()?;
        Ok(CloseReceipt { volume_id: new_volume, opened_chapter: opened, moved })
    }

    /// 根层形态：把「上一个卷之后、到这一点为止」这一段散章收成一个新卷。
    fn close_at_root(&mut self, work_id: i64, node_id: i64, title: &str) -> Result<CloseReceipt> {
        let roots = self.children_of(work_id, None)?;
        let at = roots.iter().position(|node| node.id == node_id).ok_or_else(|| {
            Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
        })?;
        let start = roots
            .iter()
            .rposition(|node| node.kind == NodeKind::Volume)
            .map(|last| last + 1)
            .unwrap_or(0);
        if at < start {
            return Err(Error::invalid(codes::VOLUME_CLOSE_POINT));
        }
        let run: Vec<i64> = roots[start..=at].iter().map(|node| node.id).collect();
        let naming = self.naming_style(work_id)?;

        // 同一个事务：要么整段收成一个卷，要么一条都不动（评审：严重 4）
        let tx = self.conn.transaction()?;
        let new_volume =
            node_edit::create_node_in(&tx, work_id, None, NodeKind::Volume, title, naming)?;
        node_edit::move_node_in(&tx, new_volume, None, start)?;
        for id in &run {
            node_edit::move_node_in(&tx, *id, Some(new_volume), usize::MAX)?;
        }
        let moved = run.len();
        Self::record_in(
            &self.device_id,
            &tx,
            "nodes",
            new_volume,
            "close_volume",
            json!({ "at_root": true, "moved": moved }),
        )?;
        tx.commit()?;
        // 光标不动：他正写的那一章还是同一个节点，只是进了新卷
        Ok(CloseReceipt { volume_id: new_volume, opened_chapter: None, moved })
    }

    /// **撤卷**：取消这一卷的分卷，把里面的东西按原顺序还回去（逆操作，见模块头）。
    pub fn dissolve_volume(&mut self, volume_id: i64) -> Result<DissolveReceipt> {
        let work_id = self.node_work(volume_id)?;
        let (parent, kind) = self.boundary_of(volume_id)?;
        if kind != NodeKind::Volume {
            return Err(Error::invalid_with(
                codes::VOLUME_NOT_VOLUME,
                [("node_id", volume_id.to_string())],
            ));
        }
        let kids: Vec<i64> =
            self.children_of(work_id, Some(volume_id))?.iter().map(|node| node.id).collect();
        let siblings = self.children_of(work_id, parent)?;
        let at = siblings.iter().position(|node| node.id == volume_id).unwrap_or(0);
        // 前面那个同父的卷（若有）：并进它的末尾——这正好还原"卷内分出新卷"那一步
        let merged_into = at
            .checked_sub(1)
            .and_then(|previous| siblings.get(previous))
            .filter(|node| node.kind == NodeKind::Volume)
            .map(|node| node.id);
        // 同一个事务：搬东西 + 软删这一卷，要么全成要么全不成（评审：严重 4）
        let tx = self.conn.transaction()?;
        match merged_into {
            Some(target) => {
                for kid in &kids {
                    node_edit::move_node_in(&tx, *kid, Some(target), usize::MAX)?;
                }
            }
            // 前面没有卷：抬到父层，占据这一卷原来的位置（还原"根层收拢散章"那一步）
            None => {
                for (offset, kid) in kids.iter().enumerate() {
                    node_edit::move_node_in(&tx, *kid, parent, at + offset)?;
                }
            }
        }
        // 软删：进回收站捞得回来——"一键撤销"反悔两次也不丢东西
        let deleted = node_edit::soft_delete_node_in(&tx, volume_id)?;
        let moved = kids.len();
        Self::record_in(
            &self.device_id,
            &tx,
            "nodes",
            volume_id,
            "dissolve_volume",
            json!({ "moved": moved, "merged_into": merged_into, "deleted": deleted }),
        )?;
        tx.commit()?;
        Ok(DissolveReceipt { moved, merged_into })
    }
}
