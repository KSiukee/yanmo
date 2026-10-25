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
