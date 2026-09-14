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
    /// 稳定代码（命令行 `--format` 用它）。
    pub const fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Text => "txt",
            ExportFormat::Json => "json",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_export_format`。
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
                // 这份 JSON 是**"最容易再读回来"**的那一份（读它的是 [`super::import`]），
                // 所以它要把"读回来需要的东西"带全，宁可多一格：
                //
                // - `naming` / `language`：标题里的号是**按位置算**的，而"还没起名的卷"要用
                //   `naming` 那一档才算得出名字（中文档给「第一卷」、补零档给「第001卷」）。
                //   少了它，读回来只能按新库的默认档渲染，成稿就跟原件对不上了；
                // - 每个节点上的 `title_template`：**只在它跟显示名不一样时才写**——
                //   作者写的是模板（`第{$N}章 灯`），显示名是渲染结果（`第3章 灯`）；
                //   只带显示名的话，读回来就成了钉死的文字（插入一章号不会重排）。
                //   空串也是有效取值（＝"没起名的卷"），所以**这一格在不在本身就是信息**。
                let payload = serde_json::json!({
                    "title": work.title,
                    "kind": work.kind.as_str(),
                    "language": work.language.as_str(),
                    "naming": self.naming_style(work_id)?.as_str(),
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

    /// 一本书分章文本成稿的**内容指纹**。
    ///
    /// 备份清单里记它、从成稿导入时对账也用它——**口径只有这一处**：
    /// 按渲染顺序把分章 txt 拼起来做摘要。两处各写一遍的话，"清单里那个指纹"和
    /// "导入时算出来的指纹"迟早不是同一个东西，对账就永远是假绿。
    pub fn draft_fingerprint(&self, work_id: i64) -> Result<String> {
        Ok(fingerprint_of_text(&self.render_work(work_id, ExportFormat::Text)?))
    }
}

/// 一组分章文本成稿的内容指纹（口径见 [`Store::draft_fingerprint`]）。
pub(crate) fn fingerprint_of_text(files: &[RenderedFile]) -> String {
    let mut combined = String::new();
    for file in files {
        combined.push_str(&String::from_utf8_lossy(&file.content));
    }
    crate::text::content_hash(&combined)
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
    if node.title_rendered.trim().is_empty() {
        node.kind.as_str().to_string()
    } else {
        safe_file_name(&node.title_rendered)
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
        // **有没有下级按数据判，不按"这种类型能不能放下级"判**：后者是界面上的可放性
        // （点「+」往哪儿加），而导出是"把作者的字带走"——一个标志位不该让它偷偷少几章。
        // 真踩过：单篇挂了一节（数据层允许），分章导出只出了单篇那一个文件，节里的字没影了。
        let container =
            node.kind.accepts_children() || kids.contains_key(&Some(node.id));
        let here = if container { join(path, &segment(node)) } else { path.to_string() };
        if node.kind.holds_body() {
            out.push(RenderedFile::text(
                format!("{}.txt", join(path, &segment(node))),
                normalize(&store.read_body(node.id)?),
            ));
        }
        if container {
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
        item.insert("title".into(), node.title_rendered.clone().into());
        // 原文与显示名不一样才写这一格（**空串也算不一样**：那是"还没起名的卷"）
        if node.title != node.title_rendered {
            item.insert("title_template".into(), node.title.clone().into());
        }
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
