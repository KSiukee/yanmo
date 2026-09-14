//! 节点树的**读**：目录树、单层懒加载、归属检查。
//!
//! - 只返回 [`NodeSummary`]（元数据，**没有正文字段**）——懒加载靠类型保证，不靠自觉；
//! - "卷 / 章 / 节 / 单篇 / 场景卡"只是 `node_kind` 的取值，**代码里不得假设层级**；
//! - 编辑（建 / 改名 / 移动 / 删除）在 [`super::node_edit`]，读写分家免得互相拖累。

use rusqlite::{params, OptionalExtension};

use super::{too_deep, Store, MAX_TREE_DEPTH};
use crate::error::{codes, Error, Result};
use crate::model::NodeKind;

/// 目录树条目：**没有正文字段**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSummary {
    pub id: i64,
    pub work_id: i64,
    pub parent_id: Option<i64>,
    pub kind: NodeKind,
    /// **作者写的原文**（含自动编号宏时就是模板，如 `第{$N}章 灯`）——**改名时编辑的就是它**
    pub title: String,
    /// **显示用的那一份**：宏已按同层位置渲染（`第3章 灯`）。
    ///
    /// 界面显示、导出、大纲一律用它；改名用 `title`（别让作者把号写死）。
    pub title_rendered: String,
    pub sort_order: i64,
    /// 预聚合字数（来自 `nodes.word_count`，不扫正文）——**按词**口径
    pub word_count: i64,
    /// 逐字（含标点）——预聚合，同样不扫正文
    pub char_count: i64,
    /// 逐字（不含标点）
    pub chars_no_punct: i64,
    /// 是否已有正文——空章一眼可见；但**正文本身不在这里**
    pub has_body: bool,
    /// 这一章的一句话（作者手填，空串＝没写过）：投稿包的大纲要按阅读顺序取它。
    ///
    /// 顺带查出来而不是单开一次查询——目录树本来就要把这棵树拉一遍，
    /// 导出/大纲再为每章各跑一趟就成了 N+1。
    pub summary: String,
    /// 下面还有没有节点（界面据此决定要不要画展开箭头）。
    ///
    /// 只问"有没有"，**不问有几个、更不问是什么**——展开箭头不该顺带把整棵子树拖出来。
    pub has_children: bool,
}

/// 树条目查询的公共部分：只取元数据 + "有没有正文 / 有没有下级" 的判断，**不取正文**。
const SUMMARY_SQL: &str = "SELECT n.id, n.work_id, n.parent_id, n.node_kind, n.title,
        n.sort_order, n.word_count, n.char_count, n.chars_no_punct,
        (c.body IS NOT NULL AND c.body <> '') AS has_body,
        EXISTS(SELECT 1 FROM nodes k WHERE k.parent_id = n.id AND k.deleted_at IS NULL) AS has_children,
        n.summary
     FROM nodes n
     JOIN works w ON w.id = n.work_id AND w.deleted_at IS NULL
     LEFT JOIN node_contents c ON c.node_id = n.id
     WHERE n.deleted_at IS NULL";

type SummaryRow = (i64, i64, Option<i64>, String, String, i64, i64, i64, i64, i64, i64, String);

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
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn build_summary(row: SummaryRow) -> Result<NodeSummary> {
    Ok(NodeSummary {
        id: row.0,
        work_id: row.1,
        parent_id: row.2,
        kind: NodeKind::parse(&row.3)?,
        title: row.4.clone(),
        // 先原样占位：编号要**按同层位置**算，等这一层都到齐了再渲染（见 super::numbering）
        title_rendered: row.4,
        sort_order: row.5,
        word_count: row.6,
        char_count: row.7,
        chars_no_punct: row.8,
        has_body: row.9 != 0,
        has_children: row.10 != 0,
        summary: row.11,
    })
}

/// 渲染一批节点的显示标题——**口径只在这一处取**：命名写法 + 章的号跨不跨卷数。
///
/// 分批、占号与跨卷延续的规则在 [`super::numbering`]（那边不认数据库之外的任何东西）。
fn render_nodes(store: &Store, nodes: &mut [NodeSummary], work_id: i64) -> Result<()> {
    let style = store.naming_style(work_id)?;
    let mode = store.chapter_numbering(work_id)?;
    let starts = super::numbering::starts_for(store.conn(), work_id, mode)?;
    super::numbering::render_titles(nodes, style, mode, &starts);
    Ok(())
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
        render_nodes(self, &mut out, work_id)?;
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
        render_nodes(self, &mut out, work_id)?;
        Ok(out)
    }

    /// **单节点的显示标题**（打开章节、界面提示这类"只要一个标题"的地方用）。
    ///
    /// 与整树渲染同一套规矩（含"跨卷延续"的起始号）：按它所在层的顺序数到它。
    pub fn rendered_title(&self, node_id: i64) -> Result<String> {
        let (work_id, parent_id, kind, title): (i64, Option<i64>, String, String) = self
            .conn
            .query_row(
                "SELECT work_id, parent_id, node_kind, title FROM nodes
                  WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
            })?;
        // ⚠️ 必须**按 id** 找自己那一行：模板可能一模一样（三章都叫 `第{$N}章`），
        // 按标题文本找会永远命中第一行——那样每章都渲染成"第1章"（压测当场抓到的）。
        let mut stmt = self.conn.prepare(
            "SELECT id, title FROM nodes
              WHERE work_id = ?1 AND parent_id IS ?2 AND node_kind = ?3 AND deleted_at IS NULL
              ORDER BY sort_order, id",
        )?;
        let rows = stmt.query_map(params![work_id, parent_id, kind], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut siblings = Vec::new();
        for row in rows {
            siblings.push(row?);
        }
        let positional = kind == crate::model::NodeKind::Volume.as_str();
        let kind_parsed = crate::model::NodeKind::parse(&kind)?;
        let style = self.naming_style(work_id)?;
        // 起始号与整树渲染同一条来路：跨卷延续时接着前面的号往下数
        let mode = self.chapter_numbering(work_id)?;
        let start =
            super::numbering::start_for_layer(&self.conn, work_id, parent_id, kind_parsed, mode)?;
        let blanks: Vec<String> = siblings
            .iter()
            .map(|(_, candidate)| {
                if positional && candidate.trim().is_empty() {
                    super::node_edit::template_for(kind_parsed, style)
                } else {
                    candidate.clone()
                }
            })
            .collect();
        let items: Vec<crate::numbering::LayerItem<'_>> = blanks
            .iter()
            .map(|candidate| crate::numbering::LayerItem { title: candidate, positional })
            .collect();
        let rendered = crate::numbering::render_layer_from(&items, start);
        let mine = siblings
            .iter()
            .position(|(id, _)| *id == node_id)
            .map(|position| rendered[position].clone());
        Ok(mine.unwrap_or(title))
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
            .ok_or_else(|| Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())]))
    }

    /// 某一章的"一句话"（投稿大纲要用它；空串＝作者没写过）。
    ///
    /// 只取这一条：打开章节时用。整棵树走 [`Store::list_nodes`]——那边一次就把每章一句话带回来了。
    pub fn node_summary(&self, node_id: i64) -> Result<String> {
        self.conn
            .query_row(
                "SELECT summary FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())]))
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
            .ok_or_else(|| Error::invalid_with(codes::NODE_GONE, [("node_id", id.to_string())]))
    }

    /// 确认节点属于指定作品——**防跨作品挂错父级**。
    pub(super) fn ensure_node_in_work(&self, node_id: i64, work_id: i64) -> Result<()> {
        let owner = self.node_work(node_id)?;
        if owner != work_id {
            return Err(Error::invalid_with(
                codes::NODE_FOREIGN_PARENT,
                [
                    ("node_id", node_id.to_string()),
                    ("owner", owner.to_string()),
                    ("work_id", work_id.to_string()),
                ],
            ));
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
            chain.push(id);
            // 祖先数 ≥ 上限 = 这一层已经比支持的还深（第 64 个祖先是上一层）。
            // 话要说准：这不是"疑似成环"。正常的一棵树到不了这里——写入口会先拒绝，
            // 只有坏数据会。
            if chain.len() >= MAX_TREE_DEPTH {
                return Err(too_deep());
            }
            current = parent_of(id)?;
        }
        chain.reverse(); // 根在前，界面照着一层层展开就行
        Ok(chain)
    }
}

/// 一棵子树的汇总：目录树里容器行要显示的"本卷几章 / 共多少字"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubtreeRollup {
    /// 子树里 `chapter` 类的节点数——"本卷几章"问的就是它（场景卡这类卡片不算章）
    pub chapters: i64,
    /// 子树里所有节点的字数之和（正文写入时已回算过，这里只是加总，**不扫正文**）——按词
    pub word_count: i64,
    /// 同上，逐字（含标点）
    pub char_count: i64,
    /// 同上，逐字（不含标点）
    pub chars_no_punct: i64,
}

impl Store {
    /// 子树汇总——**只给容器行算**（叶子行的字数是它自己那一格）。
    ///
    /// 加总的是预聚合字段 `nodes.word_count`，所以卷里有几百章也只是走一遍索引；
    /// 深度上限与别处一致：**碰到上限就明确报错，绝不静默少算**——卷行数字悄悄变少
    /// 正是最不能出的那类毛病（"说 ok 但数字不对"）。
    pub fn subtree_rollup(&self, node_id: i64) -> Result<SubtreeRollup> {
        self.node_work(node_id)?; // 顺带确认它存在且没被删
        // 顺带取回最深那一层的相对深度：它碰到上限就说明这棵树比支持的还深，
        // 这条查询已经少算了（正常的一棵树到不了，写入口会先拒绝）。
        let sql = format!(
            "WITH RECURSIVE sub(id, word_count, char_count, chars_no_punct, node_kind, depth) AS (
                 SELECT id, word_count, char_count, chars_no_punct, node_kind, 0 FROM nodes
                  WHERE parent_id = ?1 AND deleted_at IS NULL
                 UNION ALL
                 SELECT n.id, n.word_count, n.char_count, n.chars_no_punct, n.node_kind, sub.depth + 1
                   FROM nodes n
                   JOIN sub ON n.parent_id = sub.id
                  WHERE n.deleted_at IS NULL AND sub.depth < {MAX_TREE_DEPTH}
             )
             SELECT COALESCE(SUM(word_count), 0), COALESCE(SUM(char_count), 0),
                    COALESCE(SUM(chars_no_punct), 0), COALESCE(SUM(node_kind = 'chapter'), 0),
                    COALESCE(MAX(depth), 0)
               FROM sub"
        );
        let (word_count, char_count, chars_no_punct, chapters, deepest): (i64, i64, i64, i64, i64) =
            self.conn.query_row(&sql, params![node_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?;
        if deepest as usize >= MAX_TREE_DEPTH - 1 {
            return Err(too_deep());
        }
        Ok(SubtreeRollup { chapters, word_count, char_count, chars_no_punct })
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
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_NOT_BODY, [("node_id", node_id.to_string())])
            })?;
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
                    // 导航里显示的是**渲染后**的名字（`第{$N}章` → `第3章`）
                    title: node.title_rendered.clone(),
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
