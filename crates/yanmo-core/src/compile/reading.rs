//! 编译的原稿读取：**按阅读顺序走一遍目录树**，摊成编译要用的那份清单。
//!
//! 与导出那条（[`crate::store::export`]）分开：那边只关心"一章一个文件"，
//! 这边还要卷/章的层级、每章的一句话与正文段落——投稿版的大纲就是从这儿来的。
//!
//! 容器标题留空（新建的卷还没起名）时**大纲里就略过那一行**：核心不替作者编"第 N 卷"
//! 这样的句子（那是界面文案，语言一变它就过时了）。

use std::collections::HashSet;

use crate::error::Result;
use crate::store::{NodeSummary, Store};

/// 清单里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub kind: ItemKind,
    /// 标题（可能是空串：作者还没起名）
    pub title: String,
    /// 这一章的"一句话"（只有章有；空串＝没写过）
    pub summary: String,
    /// 这一章的正文段落（只有章有；已去掉空段与首尾空白）
    pub paragraphs: Vec<String>,
    /// 在树里的深度（大纲缩进用；顶层是 0）
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    /// 卷 / 节这类容器
    Container,
    /// 承载正文的章 / 单篇
    Chapter,
}

impl Item {
    /// 这一条要不要写进大纲：容器有名字才写，章一律写（标题空就报"章"这个事实）。
    pub(crate) fn outline_line(&self) -> Option<String> {
        if self.kind == ItemKind::Container {
            return (!self.title.trim().is_empty()).then(|| self.title.clone());
        }
        let title = if self.title.trim().is_empty() { "chapter".to_string() } else { self.title.clone() };
        // 一句话写在标题后面；没写就只有标题（绝不补占位句子）
        Some(match self.summary.trim() {
            "" => title,
            note => format!("{title} —— {note}"),
        })
    }
}

/// 按阅读顺序读一遍（父 → 子、sort_order → id，与导出同一条顺序）。
pub(crate) fn read(store: &Store, work_id: i64) -> Result<Vec<Item>> {
    let nodes = store.list_nodes(work_id)?;
    // 有下级的节点（按**数据**算一次，别在循环里对着整棵树反复筛）
    let parents: HashSet<i64> = nodes.iter().filter_map(|node| node.parent_id).collect();
    let mut out = Vec::new();
    collect(store, &nodes, &parents, None, 0, &mut out)?;
    Ok(out)
}

fn collect(
    store: &Store,
    nodes: &[NodeSummary],
    parents: &HashSet<i64>,
    parent: Option<i64>,
    depth: usize,
    out: &mut Vec<Item>,
) -> Result<()> {
    let mut children: Vec<&NodeSummary> =
        nodes.iter().filter(|node| node.parent_id == parent).collect();
    children.sort_by_key(|node| (node.sort_order, node.id));

    for node in children {
        let chapter = node.kind.holds_body();
        out.push(Item {
            kind: if chapter { ItemKind::Chapter } else { ItemKind::Container },
            title: node.title_rendered.clone(),
            summary: if chapter { node.summary.clone() } else { String::new() },
            paragraphs: if chapter { paragraphs(&store.read_body(node.id)?) } else { Vec::new() },
            depth,
        });
        // **有没有下级按数据判**，不按"这种类型能不能放下级"判：后者是界面上的可放性，
        // 而投稿包是"把作者的字带走"——一个标志位不该让它偷偷少几节。
        if node.kind.accepts_children() || parents.contains(&node.id) {
            collect(store, nodes, parents, Some(node.id), depth + 1, out)?;
        }
    }
    Ok(())
}

/// 正文 → 段落清单（空行分段，与正文存储口径一致）：去掉空段与首尾空白，
/// 段**内部**的换行留成空格（正文里不该有硬换行，真有了也不能把一段拆散）。
pub(crate) fn paragraphs(body: &str) -> Vec<String> {
    body.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split("\n\n")
        .map(|block| block.split('\n').map(str::trim).collect::<Vec<_>>().join(" "))
        .map(|block| block.trim().to_string())
        .filter(|block| !block.is_empty())
        .collect()
}
