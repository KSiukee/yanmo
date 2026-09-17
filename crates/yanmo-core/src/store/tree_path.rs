//! 树 → 路径：**导出与磁盘镜像共用同一套命名与走法**。
//!
//! # 为什么单独一份
//!
//! 「卷是目录、章是文件、文件名带三位序号、空标题退到结构标识、深度上限与读路径同一把尺子」
//! 是两处产物**共同的契约**。各写一遍的下场是：导出改了命名规则、镜像没跟上——同一本书
//! 在作者眼里出现两套结构，而"哪一份才算数"没人说得清。
//!
//! # 只产出路径，不碰内容
//!
//! 走一遍树、把每个节点自己的路径交给调用方，读不读正文由调用方定：
//! 导出要正文，镜像的**廉价对账**只要路径与指纹（不读正文才跑得勤）。
//!
//! # 深度上限（为什么读路径也要拦）
//!
//! 递归按 [`MAX_TREE_DEPTH`] 设了上限：比它更深的树**返回 `TREE_TOO_DEEP`，绝不递归到爆栈**。
//! 写入口早就拦着（新建/移动超深会报错），但**读路径以前没拦**——备份会对每本书都调一次渲染，
//! 一次爆栈就把整个进程带走，连关窗快照都来不及做（"不丢稿"最不该崩的就是这条路）。

use std::collections::HashMap;

use super::{too_deep, NodeSummary, MAX_TREE_DEPTH};
use crate::atomic::safe_file_name;
use crate::error::Result;

/// 按阅读顺序走一遍节点树（父 → 子、`sort_order` → `id`）。
///
/// `visit` 拿到**该节点自己的路径**（`here`，相对产品根、不含扩展名）：
/// 承载正文的节点把它当文件主干，容器（卷/节）把它当子节点的目录。
pub fn walk(
    nodes: &[NodeSummary],
    mut visit: impl FnMut(&NodeSummary, &str) -> Result<()>,
) -> Result<()> {
    let kids = children_index(nodes);
    step(nodes, &kids, None, "", 0, &mut visit)
}

fn step(
    nodes: &[NodeSummary],
    kids: &HashMap<Option<i64>, Vec<usize>>,
    parent: Option<i64>,
    dir: &str,
    ancestors: usize,
    visit: &mut impl FnMut(&NodeSummary, &str) -> Result<()>,
) -> Result<()> {
    for index in kids.get(&parent).into_iter().flatten() {
        let node = &nodes[*index];
        // 尺子与读路径**同一把**：祖先数 ≥ MAX_TREE_DEPTH 就是越限（见 `Store::node_ancestors`）。
        // 两处口径必须一致，否则会出现"读得出来、导不出来"这种反向故障。
        //
        // 这道门放在**真要处理这个节点**的时候，不能放在函数开头：容器类型（章节也算）
        // 即使没有下级也会走进来一轮空迭代——放在开头的话，写入口允许的最深那棵树
        // 会因为"空着的第 65 层"被判成坏数据（这个坑我在写这条修复时当场踩了一次）。
        if ancestors >= MAX_TREE_DEPTH {
            return Err(too_deep());
        }
        // **有没有下级按数据判，不按"这种类型能不能放下级"判**：后者是界面上的可放性
        // （点「+」往哪儿加），而导出/镜像是"把作者的字带走"——一个标志位不该让它偷偷少几章。
        // 真踩过：单篇挂了一节（数据层允许），分章导出只出了单篇那一个文件，节里的字没影了。
        let here = join(dir, &segment(node));
        visit(node, &here)?;
        if node.kind.accepts_children() || kids.contains_key(&Some(node.id)) {
            step(nodes, kids, Some(node.id), &here, ancestors + 1, visit)?;
        }
    }
    Ok(())
}

/// 节点名：标题为空（新建后还没起名）就退到**结构标识**
/// （`volume` / `chapter` …）——那是语言无关的取值，比一个"未命名"有用得多。
pub fn node_name(node: &NodeSummary) -> String {
    if node.title_rendered.trim().is_empty() {
        node.kind.as_str().to_string()
    } else {
        safe_file_name(&node.title_rendered)
    }
}

/// 节点在这条路上的名字（带序号，顺序一眼可见）。
pub fn segment(node: &NodeSummary) -> String {
    format!("{:03}-{}", node.sort_order + 1, node_name(node))
}

/// 作品目录名：`<归一化书名>-<作品 id>`。
///
/// 为什么要带 id：**重名作品是允许的**，而不带 id 的目录在两本同名书之间会互相踩——
/// 后渲染的那本会把先写下的文件当成"这次不再需要的残留"。备份那条路早就是这么防撞的。
pub fn work_folder_name(work_id: i64, work_title: &str) -> String {
    format!("{}-{}", safe_file_name(work_title), work_id)
}

pub fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}/{name}")
    }
}

/// 一份内容写成文件时的统一口径：换行归一、末尾留一个换行；空内容就是空文件。
pub(crate) fn normalize(body: &str) -> String {
    let unified = body.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = unified.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// 按 `parent_id` 归拢下标（导出那条要拼嵌套 JSON 的路也用它）。
pub(crate) fn children_index(nodes: &[NodeSummary]) -> HashMap<Option<i64>, Vec<usize>> {
    let mut kids: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (position, node) in nodes.iter().enumerate() {
        kids.entry(node.parent_id).or_default().push(position);
    }
    kids
}
