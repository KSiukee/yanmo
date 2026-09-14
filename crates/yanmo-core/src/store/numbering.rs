//! 显示标题的渲染：**号 = 位置的函数**（标题里存 `第{$N}章`，显示时按位置算）。
//!
//! 从 [`super::node`] 搬出来的：那边只管查询，渲染的规则（分层、占号、跨卷延续）
//! 是另一件变化理由；搬出来之后那一层也回到了行数上限以内。
//!
//! # 跨卷延续（`ChapterNumbering`，默认）
//!
//! - **跨卷延续**：全书按**阅读顺序**（深度优先）数下去——第1卷 1~10、第2卷 11~20；
//! - **每卷从头数**：每一卷各自从 1 开始。
//!
//! 两种都只是"渲染时怎么数"：标题里存的还是模板，所以**改一下设置立刻全见效**，
//! 正文一个字不动（导出、编译、目录树看到的都是同一份渲染结果）。
//!
//! 跨卷延续时，某一层的起始号 = 它前面（阅读顺序）所有章节数出来的结果。
//! 作者手写的 `{$N_RESET:101}` 仍然说话——那是**显式指令**，它把号拨到 101，
//! 后面的章从这里接着数（跨卷也一样）。
//!
//! # 分层与占号
//!
//! 分批到"层 + 同类"：`{$N}` 是同层序号，一层里既有卷又有章时不该互相推号。
//! 占号的两类：容器（卷 / 部，按位置排）与带计数宏的章；`序章` / `番外` 这类
//! 自起的名字不占号、也不推号。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::node::NodeSummary;
use crate::error::Result;
use crate::model::{ChapterNumbering, NamingStyle, NodeKind};
use crate::numbering;

/// 每一层（按父节点分）的**起始号**：跨卷延续时用它；每卷从头数时是空表。
pub(super) type LayerStarts = HashMap<Option<i64>, i64>;

/// 按当前编号方式准备"每层的起始号"（每卷从头数就不用算）。
pub(super) fn starts_for(
    conn: &Connection,
    work_id: i64,
    mode: ChapterNumbering,
) -> Result<LayerStarts> {
    match mode {
        ChapterNumbering::PerVolume => Ok(LayerStarts::new()),
        ChapterNumbering::Continue => layer_starts(conn, work_id),
    }
}

/// 走一遍阅读顺序，给"每一层的第一张章"记下**它进来时计数器是几**。
///
/// 只读元数据（id / 父 / 类型 / 标题），**不碰正文**；深度优先的父在子先顺序就是阅读顺序。
/// 坏数据（自环 / 互为父子）到不了根，走不到就自然停下——不会有死循环。
fn layer_starts(conn: &Connection, work_id: i64) -> Result<LayerStarts> {
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, node_kind, title FROM nodes
          WHERE work_id = ?1 AND deleted_at IS NULL
          ORDER BY parent_id, sort_order, id",
    )?;
    type Row = (i64, Option<i64>, String, String);
    let rows = stmt.query_map(params![work_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut children: HashMap<Option<i64>, Vec<Row>> = HashMap::new();
    for row in rows {
        let (id, parent, kind, title) = row?;
        children.entry(parent).or_default().push((id, parent, kind, title));
    }

    let mut starts = LayerStarts::new();
    let mut counter = 1i64;
    let mut stack: Vec<Row> = children
        .get(&None)
        .map(|roots| roots.iter().rev().cloned().collect())
        .unwrap_or_default();
    while let Some((id, parent, kind, title)) = stack.pop() {
        if NodeKind::parse(&kind)? == NodeKind::Chapter {
            // 先记"进来时是几"，再看这一章自己有没有把号拨走 / 占不占号
            starts.entry(parent).or_insert(counter);
            if let Some(start) = numbering::reset_at(&title) {
                counter = start;
            }
            if numbering::has_counter(&title) {
                counter += 1;
            }
        }
        if let Some(kids) = children.get(&Some(id)) {
            for kid in kids.iter().rev() {
                stack.push(kid.clone());
            }
        }
    }
    Ok(starts)
}

/// 渲染一批节点的显示标题（原地改 `title_rendered`）。
///
/// `nodes` 是同一本书的一批条目；顺序不影响结果（内部会按"层 + 同类"分批）。
pub(super) fn render_titles(
    nodes: &mut [NodeSummary],
    style: NamingStyle,
    mode: ChapterNumbering,
    starts: &LayerStarts,
) {
    let mut seen: Vec<(i64, Option<i64>, String)> = Vec::new();
    for node in nodes.iter() {
        let key = (node.work_id, node.parent_id, node.kind.as_str().to_string());
        if !seen.contains(&key) {
            seen.push(key);
        }
    }
    for key in seen {
        let members: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                (node.work_id, node.parent_id, node.kind.as_str().to_string()) == key
            })
            .map(|(at, _)| at)
            .collect();
        // 起始号：跨卷延续**只对章**生效（卷自己仍旧按位置排、每层从 1 数）
        let is_chapter = nodes[members[0]].kind == NodeKind::Chapter;
        let start = match mode {
            ChapterNumbering::Continue if is_chapter => {
                starts.get(&key.1).copied().unwrap_or(1)
            }
            _ => 1,
        };
        let rendered = render_layer(nodes, &members, style, start);
        for (slot, at) in members.iter().enumerate() {
            nodes[*at].title_rendered = rendered[slot].clone();
        }
    }
}

/// **某一层的起始号**：跨卷延续时是"这一层前面已经数到几"，否则是 1。
///
/// 只要一个标题的地方（打开章节、界面提示）用它——与整树渲染同一套规矩，
/// 免得"树上是第11章、打开却显示第1章"。
pub(super) fn start_for_layer(
    conn: &Connection,
    work_id: i64,
    parent_id: Option<i64>,
    kind: NodeKind,
    mode: ChapterNumbering,
) -> Result<i64> {
    match mode {
        ChapterNumbering::Continue if kind == NodeKind::Chapter => {
            Ok(layer_starts(conn, work_id)?.get(&parent_id).copied().unwrap_or(1))
        }
        _ => Ok(1),
    }
}

/// 一层（同父同类）的渲染：空名字的容器补成模板，再交给 [`crate::numbering`]。
fn render_layer(
    nodes: &[NodeSummary],
    members: &[usize],
    style: NamingStyle,
    start: i64,
) -> Vec<String> {
    // 空名字的容器 = "还没起名"（建书时留白的那一卷）：用模板渲染成 `第1卷`，
    // 于是"第几卷"只在核心这一处算——界面不用再自己补一份占位（两处算迟早不一致）。
    let blanks: Vec<String> = members
        .iter()
        .map(|at| {
            let node = &nodes[*at];
            let positional = node.kind == NodeKind::Volume;
            if positional && node.title.trim().is_empty() {
                super::node_edit::template_for(node.kind, style)
            } else {
                node.title.clone()
            }
        })
        .collect();
    let items: Vec<numbering::LayerItem<'_>> = members
        .iter()
        .enumerate()
        .map(|(slot, at)| numbering::LayerItem {
            title: &blanks[slot],
            // 容器（卷 / 部）按位置排；叶子章只有带计数宏才占号（见 numbering 的说明）
            positional: nodes[*at].kind == NodeKind::Volume,
        })
        .collect();
    numbering::render_layer_from(&items, start)
}
