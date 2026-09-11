//! 目录树命令：卷 / 章的**单层懒加载**与结构编辑。
//!
//! 分工与编辑类命令一致：界面只说"要哪一层""把谁挪到哪"，**SQL 与规则全在核心**。
//!
//! - **一层一问**：展开哪一层就问哪一层，拿回来的条目里**没有正文字段**
//!   （核心的 `NodeSummary` 类型上就没有正文，不是靠界面自觉）；
//! - **每章字数直接读**：核心写入正文时已回算过（见 `yanmo_core::store`），
//!   目录树不需要为了显示字数去扫正文；
//! - 新建章节的「+」走核心既有的"插在当前章之后"（同级、序号密集、不跨卷）。

use serde::Serialize;
use tauri::State;

use crate::storage::AppData;
use yanmo_core::model::NodeKind;
use yanmo_core::store::{NodeSummary, Store, SubtreeRollup};

/// 目录树的一个条目。
#[derive(Debug, Serialize)]
pub struct TreeNodeDto {
    pub id: i64,
    pub parent_id: Option<i64>,
    /// 「卷 / 章 / 节 / 单篇 / 场景卡」——**界面不假设层级**，只按这个取值显示
    pub kind: String,
    pub title: String,
    pub word_count: i64,
    /// 空章一眼可见
    pub has_body: bool,
    /// 这个节点**能不能编辑正文**（点一下是"打开来写"还是"展开看看"）
    pub holds_body: bool,
    /// 这个节点**能不能收下级**（拖进来算不算数）
    pub accepts_children: bool,
    /// 下面还有没有节点（决定要不要画展开箭头）
    pub has_children: bool,
    /// 容器行的小字：本卷几章 / 共多少字（**只有容器才填，叶子是 0**）
    pub chapter_count: i64,
    pub subtree_word_count: i64,
}

fn to_dto(node: NodeSummary, rollup: SubtreeRollup) -> TreeNodeDto {
    TreeNodeDto {
        id: node.id,
        parent_id: node.parent_id,
        kind: node.kind.as_str().to_string(),
        title: node.title,
        word_count: node.word_count,
        has_body: node.has_body,
        holds_body: node.kind.holds_body(),
        accepts_children: node.kind.accepts_children(),
        has_children: node.has_children,
        chapter_count: rollup.chapters,
        subtree_word_count: rollup.word_count,
    }
}

/// 取某一层的子节点——展开哪一层拉哪一层。
///
/// 容器行顺带带上"本卷几章 / 共多少字"：**只给容器算**，叶子行是自己的字数，不必多问一口。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_children(
    data: State<'_, AppData>,
    work_id: i64,
    parent_id: Option<i64>,
) -> Result<Vec<TreeNodeDto>, String> {
    data.with_store(|store: &mut Store| {
        let nodes = store.children_of(work_id, parent_id)?;
        let mut out = Vec::with_capacity(nodes.len());
        for node in nodes {
            let rollup = if node.kind.accepts_children() && !node.kind.holds_body() {
                store.subtree_rollup(node.id)?
            } else {
                SubtreeRollup::default()
            };
            out.push(to_dto(node, rollup));
        }
        Ok(out)
    })
}

/// 从根到该节点父级的 id 链——界面用它**一层层展开到正在写的那一章**。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_ancestors(data: State<'_, AppData>, node_id: i64) -> Result<Vec<i64>, String> {
    data.with_store(|store: &mut Store| store.node_ancestors(node_id))
}

/// 在指定位置新建节点（卷 / 章 / …），返回新节点的 id。
///
/// **标题留空 = 按同层取号自动命名**（`第N章` 之类），规则在核心那一份实现里。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_create_node(
    data: State<'_, AppData>,
    work_id: i64,
    parent_id: Option<i64>,
    kind: String,
    title: String,
) -> Result<i64, String> {
    data.with_store(|store: &mut Store| {
        store.create_node(work_id, parent_id, NodeKind::parse(&kind)?, &title)
    })
}

/// 内联改名。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_rename_node(
    data: State<'_, AppData>,
    node_id: i64,
    title: String,
) -> Result<(), String> {
    data.with_store(|store: &mut Store| store.rename_node(node_id, &title))
}

/// 拖拽排序：挪到新父级的第 `index` 位（越界夹到末尾，两侧同级重排成密集序号）。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_move_node(
    data: State<'_, AppData>,
    node_id: i64,
    parent_id: Option<i64>,
    index: i64,
) -> Result<(), String> {
    data.with_store(|store: &mut Store| {
        store.move_node(node_id, parent_id, index.max(0) as usize)
    })
}

/// 删掉一个节点（**软删除**：连同子树一起进回收站，之后能捞回来）。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_delete_node(data: State<'_, AppData>, node_id: i64) -> Result<usize, String> {
    data.with_store(|store: &mut Store| store.soft_delete_node(node_id))
}

/// 每卷目标章数（作者自己定的"大概几章一卷"）：目录里那行"本卷 12/30 章"的分母。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_volume_target(data: State<'_, AppData>, work_id: i64) -> Result<Option<i64>, String> {
    data.with_store(|store: &mut Store| store.volume_target(work_id))
}

/// 设定 / 清除每卷目标章数：`null`（或 ≤0）= 清掉，回到"没设过"。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_set_volume_target(
    data: State<'_, AppData>,
    work_id: i64,
    chapters: Option<i64>,
) -> Result<(), String> {
    data.with_store(|store: &mut Store| store.set_volume_target(work_id, chapters))
}
