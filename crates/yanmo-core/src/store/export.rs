//! 导出：把一本书**渲染成一组文件**——纯函数，**不碰文件系统**。
//!
//! 分两层是有意的：
//! - 这一层只管"应该有哪些文件、每个文件里是什么"（可单测、可复现）；
//! - 落盘（原子写、清掉上一次的残留）在壳里，见 `yanmo_app::storage`。
//!
//! 这样同一份渲染，既能用于"导出给作者带走"，也能用于将来"磁盘镜像"那件事。
//!
//! # 幂等
//!
//! **同一份内容、同一份结构，渲染出来的字节必须完全一样**：不写时间戳、不写绝对路径、
//! 顺序一律按阅读顺序（父 → 子、sort_order → id），换行统一成 `\n` 并以一个换行结尾。
//! 这样把它放进 git，diff 里只会出现真正的改动——不会因为"又导了一次"冒出噪声。
//!
//! # 结构怎么表达
//!
//! 只输出**文件**：卷 / 节这些容器体现在路径里（`001-第一卷/002-第一章.txt`），
//! 由落盘那一层按需建目录。文件名带三位序号，所以文件管理器里的顺序就是阅读顺序。

use std::collections::HashMap;

use super::Store;
use crate::atomic::safe_file_name;
use crate::error::{codes, Error, Result};

/// 导出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// 分章纯文本（一章一个文件，结构用目录表示）——最容易被人接手
    Text,
    /// 单个 JSON（结构与正文都在里面）——最容易再读回来
    Json,
}

impl ExportFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Text => "txt",
            ExportFormat::Json => "json",
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "txt" | "text" => Ok(ExportFormat::Text),
            "json" => Ok(ExportFormat::Json),
            other => Err(Error::invalid_with(
                codes::UNKNOWN_EXPORT_FORMAT,
                [("value", other.to_string())],
            )),
        }
    }
}

/// 一个要写出去的文件：路径一律是**相对导出目录**的（别把机器的绝对路径带进去）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFile {
    pub relative_path: String,
    /// 文件内容一律是**字节**：文本与二进制（`docx` 那种打包文件）走同一条渲染 → 落盘通道，
    /// 落盘那边不必分两套写法。
    pub content: Vec<u8>,
}

impl RenderedFile {
    /// 文本产物：按 UTF-8 编成字节（换行归一与"末尾一个换行"由调用方的口径保证）。
    pub fn text(relative_path: impl Into<String>, content: String) -> Self {
        Self { relative_path: relative_path.into(), content: content.into_bytes() }
    }
}

impl Store {
    /// 把一本书渲染成一组文件。
    pub fn render_work(&self, work_id: i64, format: ExportFormat) -> Result<Vec<RenderedFile>> {
        let work = self.get_work(work_id)?;
        let nodes = self.list_nodes(work_id)?;
        let kids = children_index(&nodes);
        match format {
            ExportFormat::Text => {
                let mut out = Vec::new();
                collect_text(self, &nodes, &kids, None, "", &mut out)?;
                if out.is_empty() {
                    // 一个字都没有的书：留一个文件，免得导出一个空文件夹让人以为失败了。
                    // 文件名**语言无关**（它会留在作者磁盘上，不该随界面语言变）
                    out.push(RenderedFile::text("empty.txt", normalize(&work.title)));
                }
                Ok(out)
            }
            ExportFormat::Json => {
                let payload = serde_json::json!({
                    "title": work.title,
                    "kind": work.kind.as_str(),
                    "nodes": json_nodes(self, &nodes, &kids, None)?,
                });
                Ok(vec![RenderedFile::text(
                    "work.json",
                    // 固定缩进 + 不写时间戳：同样的内容永远渲染出同样的字节
                    format!("{}\n", serde_json::to_string_pretty(&payload).unwrap_or_default()),
                )])
            }
        }
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

fn children_index(nodes: &[super::NodeSummary]) -> HashMap<Option<i64>, Vec<usize>> {
    let mut kids: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (position, node) in nodes.iter().enumerate() {
        kids.entry(node.parent_id).or_default().push(position);
    }
    kids
}

/// 节点名：标题为空（新建后还没起名）就退到**结构标识**
/// （`volume` / `chapter` …）——那是语言无关的取值，比一个"未命名"有用得多。
fn node_name(node: &super::NodeSummary) -> String {
    if node.title.trim().is_empty() {
        node.kind.as_str().to_string()
    } else {
        safe_file_name(&node.title)
    }
}

/// 节点在这条路上的名字（带序号，顺序一眼可见）。
fn segment(node: &super::NodeSummary) -> String {
    format!("{:03}-{}", node.sort_order + 1, node_name(node))
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}/{name}")
    }
}

/// 按阅读顺序走一遍，把承载正文的节点收成文件（容器只体现在路径里）。
fn collect_text(
    store: &Store,
    nodes: &[super::NodeSummary],
    kids: &HashMap<Option<i64>, Vec<usize>>,
    parent: Option<i64>,
    path: &str,
    out: &mut Vec<RenderedFile>,
) -> Result<()> {
    for index in kids.get(&parent).into_iter().flatten() {
        let node = &nodes[*index];
        let here = if node.kind.accepts_children() {
            join(path, &segment(node))
        } else {
            path.to_string()
        };
        if node.kind.holds_body() {
            out.push(RenderedFile::text(
                format!("{}.txt", join(path, &segment(node))),
                normalize(&store.read_body(node.id)?),
            ));
        }
        if node.kind.accepts_children() {
            collect_text(store, nodes, kids, Some(node.id), &here, out)?;
        }
    }
    Ok(())
}

fn json_nodes(
    store: &Store,
    nodes: &[super::NodeSummary],
    kids: &HashMap<Option<i64>, Vec<usize>>,
    parent: Option<i64>,
) -> Result<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    for index in kids.get(&parent).into_iter().flatten() {
        let node = &nodes[*index];
        let mut item = serde_json::Map::new();
        item.insert("kind".into(), node.kind.as_str().into());
        item.insert("title".into(), node.title.clone().into());
        if node.kind.holds_body() {
            item.insert("body".into(), normalize(&store.read_body(node.id)?).into());
        }
        let children = json_nodes(store, nodes, kids, Some(node.id))?;
        if !children.is_empty() {
            item.insert("children".into(), children.into());
        }
        out.push(serde_json::Value::Object(item));
    }
    Ok(out)
}
