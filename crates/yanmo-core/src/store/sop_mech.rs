//! 机械检查项**要读的那点事实**：本章一条、全书一条。
//!
//! 判据一条都不在这里——全部复用 `crate::outline::actual`（认人最长优先 / 伏笔候选），
//! 这里只把值读齐、数一数。这与「计划 vs 实际」那一屏**同一套判据**，所以两处永远说得一样；
//! 要改判据只改 `outline/actual.rs` 一处。
//!
//! # 两种视角，两个成本
//!
//! | 视角 | 读什么 | 成本 |
//! |---|---|---|
//! | 本章（[`Store::chapter_mech_facts`]）| 只读**这一章**的正文 + 这本书的卡与伏笔 | 一次翻页 |
//! | 全书（[`Store::book_mech_facts`]）| 复用 [`Store::outline_actuals`]（要读**每一章**正文）| 一次全书扫描 |
//!
//! 所以界面上**按需取**：这一份流程里没有全书视角的机械项，就别去扫全书。
//!
//! # 核心零文案
//!
//! 这里只说事实（缺了谁、几章有问题），一个句子都不拼——句子在界面字典里。

use super::Store;
use crate::error::Result;
use crate::model::ForeshadowState;
use crate::outline::{foreshadow_candidates, mentioned_cards, CastRef, ForeshadowHit};

/// 当前这一章的读数要的事实。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChapterMechFacts {
    pub node_id: i64,
    /// 显示用的章名（宏已渲染的那一份）
    pub title: String,
    /// 这一章有正文吗。
    ///
    /// 没动笔的章**不判"缺人"**——正文是空的，计划里挂着的人当然一个都认不到，
    /// 那是"还没轮到它"，不是"写漏了"（与 `outline::actual::chapter_state`
    /// 里"两样都空就不报"同一条分寸）。界面拿它说"还没动笔"。
    pub has_body: bool,
    /// 计划里挂了几个人
    pub planned: usize,
    /// 正文里认到了几个（**计划里的**那几个）
    pub matched: usize,
    /// 计划里挂着、正文里没认到的（只提示，绝不自动撤）
    pub missing: Vec<CastRef>,
    /// 正文里出现、计划里没挂的（可以一键补进计划）
    pub extra: Vec<CastRef>,
    /// 全书还埋着几条伏笔（一条都没有时，"伏笔点到没"这件事无从谈起）
    pub planted_open: usize,
    /// 像在这一章被点到的伏笔（**候选**，不是结论）
    pub foreshadows: Vec<ForeshadowHit>,
}

/// 整本书的读数要的事实。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BookMechFacts {
    /// 对到了几章（判过的那几章；没计划也没正文的章不在里头）
    pub chapters: usize,
    /// 书太大时没对到的章数（如实报，别假装全书都看了）
    pub truncated: usize,
    /// 有几章"计划里挂着的人没认到"
    pub cast_missing: usize,
    /// 有几章"正文里出现了计划外的人"
    pub cast_extra: usize,
    /// 有几章"还埋着的伏笔像被点到了"
    pub foreshadow: usize,
    /// 全书还埋着几条伏笔
    pub planted_open: usize,
}

impl Store {
    /// 当前这一章的读数（**只读**，也只读这一章）。
    pub fn chapter_mech_facts(&self, node_id: i64) -> Result<ChapterMechFacts> {
        let work_id = self.node_work(node_id)?;
        let title = self.rendered_title(node_id)?;
        let body = self.read_body(node_id)?;
        // 卡与伏笔一次读全：只为一章各读一遍，等于把"这本书有多少卡"变成 N 次查询
        let cards = self.entity_cards(work_id, None)?;
        let foreshadows = self.foreshadows(work_id, None)?;

        let planned: Vec<CastRef> = self
            .node_cast_of(node_id)?
            .into_iter()
            .map(|member| CastRef {
                entity_id: member.entity_id,
                name: member.name.clone(),
                matched: member.name,
            })
            .collect();
        let mentioned = mentioned_cards(&body, &cards);
        let missing: Vec<CastRef> = planned
            .iter()
            .filter(|plan| !mentioned.iter().any(|found| found.entity_id == plan.entity_id))
            .cloned()
            .collect();
        let extra: Vec<CastRef> = mentioned
            .iter()
            .filter(|found| !planned.iter().any(|plan| plan.entity_id == found.entity_id))
            .cloned()
            .collect();
        let matched = mentioned
            .iter()
            .filter(|found| planned.iter().any(|plan| plan.entity_id == found.entity_id))
            .count();

        Ok(ChapterMechFacts {
            node_id,
            title,
            has_body: !body.is_empty(),
            planned: planned.len(),
            matched,
            missing,
            extra,
            planted_open: planted_open(&foreshadows),
            foreshadows: foreshadow_candidates(&body, &foreshadows),
        })
    }

    /// 整本书的读数（**只读**；要读每一章正文，所以这是界面上的一次显式动作）。
    pub fn book_mech_facts(&self, work_id: i64) -> Result<BookMechFacts> {
        let report = self.outline_actuals(work_id)?;
        let foreshadows = self.foreshadows(work_id, None)?;
        Ok(BookMechFacts {
            chapters: report.chapters.len(),
            truncated: report.truncated,
            cast_missing: report.chapters.iter().filter(|c| !c.missing.is_empty()).count(),
            cast_extra: report.chapters.iter().filter(|c| !c.extra.is_empty()).count(),
            foreshadow: report.chapters.iter().filter(|c| !c.foreshadows.is_empty()).count(),
            planted_open: planted_open(&foreshadows),
        })
    }
}

/// 还**埋着**的伏笔有几条（收掉的不算——不再念）。
fn planted_open(foreshadows: &[crate::model::Foreshadow]) -> usize {
    foreshadows.iter().filter(|item| item.state == ForeshadowState::Planted).count()
}
