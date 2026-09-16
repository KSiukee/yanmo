//! 大纲表要读的那一屏：**整棵树按阅读顺序铺平**，每一行带上该有的列。
//!
//! 表里一行＝一个节点（卷 / 章 / 节 / 场景卡），缩进由 `depth` 给——界面照着摆，
//! **不必自己再算层级**（层级只有一个来源：树的走法，见 [`Store::node_walk`]）。
//!
//! 三件事在这一层拼好，界面就不用为一屏发四次请求：
//!
//! 1. **四格**（视角 / 目标 / 冲突 / 结果）——凡承载正文的节点都有，没填就是空串；
//! 2. **伏笔账**——这一章埋着几条还没收、收掉几条（作者一眼看得出哪一章的线头没结）；
//! 3. **字数与有没有正文**——目录树本来就读它们，顺手带上；
//! 4. **出场人物**——这一章挂了哪几张人物卡（名字是读的这一刻从卡上取的，
//!    见 [`Store::node_cast`]）。
//!
//! 它是**只读**的：改哪一格走 [`Store::save_scene_fields`] / [`Store::set_node_summary`]，
//! 一次改一个节点的那一项——表格逐格存，不该整屏重写；整片粘进来的那一片走
//! [`Store::save_outline_cells`]（一片一次事务，理由在 `store::outline_paste`）。

use std::collections::HashMap;

use super::{CastMember, Store};
use crate::error::Result;
use crate::model::{ForeshadowState, NodeKind, SceneFields};

/// 大纲表里的一行。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OutlineRow {
    pub node_id: i64,
    pub kind: NodeKind,
    /// 父节点（界面据此做"展开 / 折叠"那一层：卷下面才是章，章下面才是场景卡）
    pub parent_id: Option<i64>,
    /// 缩进层级（0 = 卷这一层）
    pub depth: usize,
    /// **渲染后**的名字（`第{$N}章` → `第3章`）；空串＝作者还没起名
    pub title: String,
    /// 这一章的"一句话"（章纲）
    pub summary: String,
    pub word_count: i64,
    /// 有没有正文（空章一眼可见）
    pub has_body: bool,
    /// 四格（没填过的节点是四个空串）
    pub fields: SceneFields,
    /// 这一章**埋着还没收**的伏笔有几条
    pub planted_open: usize,
    /// 这一章**收掉**的伏笔有几条
    pub collected: usize,
    /// 这一段出场的人物（空表＝还没挂过；名字是刚才从设定卡上读的）
    pub cast: Vec<CastMember>,
}

impl Store {
    /// 这本书的大纲表（**整棵树铺平**，按阅读顺序）。
    pub fn outline_rows(&self, work_id: i64) -> Result<Vec<OutlineRow>> {
        // 四格：一次拿全（只有填过的节点才有行）
        let mut fields: HashMap<i64, SceneFields> = HashMap::new();
        for (node_id, _title, scene) in self.nodes_with_fields(work_id)? {
            fields.insert(node_id, scene);
        }
        // 伏笔账：按"埋在哪一章 / 收在哪一章"数
        let mut planted_open: HashMap<i64, usize> = HashMap::new();
        let mut collected: HashMap<i64, usize> = HashMap::new();
        for item in self.foreshadows(work_id, None)? {
            if item.state == ForeshadowState::Planted {
                if let Some(node) = item.planted_node {
                    *planted_open.entry(node).or_default() += 1;
                }
            }
            if let Some(node) = item.collected_node {
                *collected.entry(node).or_default() += 1;
            }
        }

        let mut rows = Vec::new();
        let mut cast = self.node_cast(work_id)?;
        for (node, depth) in self.node_walk(work_id)? {
            rows.push(OutlineRow {
                node_id: node.id,
                kind: node.kind,
                parent_id: node.parent_id,
                depth,
                title: node.title_rendered,
                summary: node.summary,
                word_count: node.word_count,
                has_body: node.has_body,
                fields: fields
                    .remove(&node.id)
                    .unwrap_or_else(|| SceneFields::empty(node.id)),
                planted_open: planted_open.get(&node.id).copied().unwrap_or(0),
                collected: collected.get(&node.id).copied().unwrap_or(0),
                // 挂过人的节点才在表里（没挂过就是空名单——"没填"与"填成空"是同一件事）
                cast: cast.remove(&node.id).unwrap_or_default(),
            });
        }
        Ok(rows)
    }
}
