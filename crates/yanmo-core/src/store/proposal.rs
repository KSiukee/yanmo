//! 模块的「提案权」：**不给写，给建议**——作者永远是唯一的应用者。
//!
//! # 为什么是提案不是写
//!
//! 稿子是用户的命根子：模块直写时，一个 bug（不需要恶意）就够酿灾难；提案形态下
//! bug 只表现为"一份奇怪的 diff"，作者看一眼就拒了。而且**写权限给了收不回来**，
//! "建议"这个口子随时可以收紧。
//!
//! # 乐观并发：哈希对不上就拒
//!
//! 每一份提案都带着"它是基于哪一版正文提出的"（`base_hash`）。作者在这期间改过
//! 稿子，哈希就对不上——这时**拒绝**并明确说"这个建议基于旧版本，已失效"，
//! 绝不静默套用（静默套用等于用旧内容覆盖新内容）。
//!
//! # 应用走哪条路
//!
//! 不新写一条写路径：校验通过后直接走既有的 [`Store::write_body`]（原子 + 已有统计
//! 与来源口径）。提案只负责"说什么"，落盘仍是核心那一条路。

use serde::Serialize;

use super::Store;
use crate::error::Result;

/// 应用一份提案的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProposalOutcome {
    /// 应用了（带落盘后的字数）。
    Applied { chars: u64 },
    /// 失效：稿子在提案之后被改过（带当前哈希，界面可以提示"基于旧版本"）。
    Stale { current_hash: String },
    /// 目标不在了（章被删了）。
    Missing,
}

impl ProposalOutcome {
    pub fn applied(&self) -> bool {
        matches!(self, ProposalOutcome::Applied { .. })
    }
}

/// 一次**树结构**改动（提案里的操作：改名 / 移动）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeChange {
    /// 改名
    Rename { title: String },
    /// 移到某个父节点下（`None` = 根层）；`index` = 落点下标
    Move { parent: Option<i64>, index: usize },
}

impl Store {
    /// 一段正文当前的哈希（给提案带上"基于哪一版"；段不存在 = `None`）。
    pub fn body_hash(&self, node_id: i64) -> Result<Option<String>> {
        // 段不在了（删了 / 从没存在）就没有"当前版本"可言。
        if self.node_summary(node_id).is_err() {
            return Ok(None);
        }
        let hash: Option<String> = self
            .conn
            .query_row(
                "SELECT content_hash FROM node_contents WHERE node_id = ?1",
                [node_id],
                |row| row.get(0),
            )
            .ok();
        Ok(hash)
    }

    /// 一个节点的"树指纹"（父节点 + 标题）：树提案靠它判断"作者这期间动过没有"。
    pub fn tree_fingerprint(&self, node_id: i64) -> Result<Option<String>> {
        let row: Option<(Option<i64>, String)> = self
            .conn
            .query_row(
                "SELECT parent_id, title FROM nodes WHERE id = ?1 AND deleted_at IS NULL",
                [node_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        Ok(row.map(|(parent, title)| {
            format!("{}|{}", parent.map(|id| id.to_string()).unwrap_or_default(), title)
        }))
    }

    /// 应用一份**树结构提案**（改名 / 移动）：指纹对不上就拒，绝不覆盖作者的动作。
    pub fn apply_tree_proposal(
        &mut self,
        node_id: i64,
        base_fingerprint: &str,
        change: &TreeChange,
    ) -> Result<ProposalOutcome> {
        let Some(current) = self.tree_fingerprint(node_id)? else {
            return Ok(ProposalOutcome::Missing);
        };
        if base_fingerprint.trim().is_empty() || base_fingerprint.trim() != current {
            return Ok(ProposalOutcome::Stale { current_hash: current });
        }
        match change {
            TreeChange::Rename { title } => {
                let title = title.trim();
                if title.is_empty() {
                    return Ok(ProposalOutcome::Missing);
                }
                self.rename_node(node_id, title)?;
            }
            TreeChange::Move { parent, index } => {
                self.move_node(node_id, *parent, *index)?;
            }
        }
        Ok(ProposalOutcome::Applied { chars: 0 })
    }

    /// 应用一份**正文提案**：哈希对不上就拒，绝不覆盖作者的新内容。
    ///
    /// `base_hash` 为空表示"提案没带依据"——也拒（没有依据的提案没法判断新旧）。
    pub fn apply_text_proposal(
        &mut self,
        node_id: i64,
        base_hash: &str,
        body: &str,
    ) -> Result<ProposalOutcome> {
        let Some(current) = self.body_hash(node_id)? else {
            return Ok(ProposalOutcome::Missing);
        };
        if base_hash.trim().is_empty() || base_hash.trim() != current {
            return Ok(ProposalOutcome::Stale { current_hash: current });
        }
        let stats = self.write_body(node_id, body)?;
        Ok(ProposalOutcome::Applied { chars: stats.char_count.max(0) as u64 })
    }
}
