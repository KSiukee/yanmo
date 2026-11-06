// i18n-allow-file: 这是随交稿附给编辑看的一页纸（不是界面文案；界面字典不分发到包外）
//! 来源清单：交稿时可以附的一页「每章的字从哪儿来」（**只有字数，没有正文**）。
//!
//! 与状态栏那行读数是同一份账（`node_source_runs`），区别只在"给谁看"：
//! 状态栏给作者自己看，这一页给编辑 / 平台看。它**不判断**任何一档的好坏——
//! 「未记录」只是"没法归因"，不是"可疑"；这话要写在清单里，免得被人拿去做文章。
//!
//! 口径与 `source_breakdown` 完全一致（同一份段、同一套字数），不另算一套。

use super::Store;
use crate::error::Result;
use crate::model::{EditOrigin, NodeKind};

/// 交稿时附在产物旁边的文件名（壳与命令行共用这一份）。
pub const SOURCE_REPORT_FILE: &str = "来源清单.txt";

/// 稳定码 → 人话（只在**这一页纸**上用；界面文案仍住字典里）。
fn label(code: &str) -> &'static str {
    match code {
        "typed" => "键入",
        "pasted" => "粘贴",
        "module" => "模块",
        "voice" => "口述",
        "system" => "软件写入",
        "mixed" => "混合",
        _ => "未记录",
    }
}

impl Store {
    /// 生成这份清单的正文（纯文本）。**只读**：不改库、不写文件。
    pub fn source_report_text(&self, work_id: i64) -> Result<String> {
        let work = self.get_work(work_id)?;
        let nodes = self.list_nodes(work_id)?;

        let mut lines = vec![
            "研墨 · 来源清单".to_string(),
            "================".to_string(),
            String::new(),
            format!("书：《{}》", work.title),
            String::new(),
            "说明：以下是每章正文的「字从哪儿来」统计——只有字数，不含正文内容。".to_string(),
            "「未记录」是没法归因的字（老稿、导入、软件写入等），不代表任何判断。".to_string(),
            "这份清单同样不能证明「这稿是谁写的」；它只如实记录过程。".to_string(),
            String::new(),
        ];

        let mut total_chars = 0i64;
        let mut total_typed = 0i64;
        for node in &nodes {
            if !matches!(node.kind, NodeKind::Chapter | NodeKind::Section) {
                continue;
            }
            let breakdown = self.source_breakdown(node.id)?;
            if breakdown.shares.is_empty() {
                continue; // 空章没有字，也就没有"来源"
            }
            let chars: i64 = breakdown.shares.iter().map(|share| share.char_count).sum();
            total_chars += chars;
            total_typed += breakdown.chars_of(EditOrigin::Typed);

            let title = if node.title.contains("{$") {
                self.rendered_title(node.id).unwrap_or_else(|_| node.title.clone())
            } else {
                node.title.clone()
            };
            let parts: Vec<String> = breakdown
                .shares
                .iter()
                .map(|share| format!("{} {}", label(&share.source), share.char_count))
                .collect();
            lines.push(format!("{title}    {chars} 字（{}）", parts.join(" · ")));
        }

        lines.push(String::new());
        lines.push(format!(
            "合计：{total_chars} 字（其中键入 {total_typed} 字）"
        ));
        lines.push(String::new());
        Ok(lines.join("\n"))
    }
}
