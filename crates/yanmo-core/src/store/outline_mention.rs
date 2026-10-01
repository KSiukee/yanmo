//! 「正文里冒出来的名字」要读的那点数据：**全书扫一遍正文**，交给纯逻辑那一层。
//!
//! 判据在 [`crate::outline::mention`]（纯函数：认名字、跟设定卡比对、门槛与上限），
//! 这里只负责把值读齐（正文与设定卡的读法只有这几行）。
//!
//! # 为什么**不**挂在打开面板就自动跑的那条扫描上
//!
//! 其余六条规则全靠结构化数据（几张小表），一次扫描是毫秒级，所以 `outline_scan`
//! 在面板上自动跑。这一条要**读每一章正文**——与"计划 vs 实际"同一档成本，
//! 那是界面上的一次**显式动作**（按需取，见 `store::sop_mech` 里那张成本表）。
//! 挂上去的后果是：作者每切到这一栏就等一次全书扫描，而他多半只是来看一眼伏笔账。
//!
//! # 只读
//!
//! 扫一遍不动一个字节。真会写的是两条界面上给出的出口：**建成卡**（走既有
//! `entity_create`）与**"这不是名字"**（走既有的忽略标记）。

use super::Store;
use crate::error::Result;
use crate::outline::{unlisted_name_issues, unlisted_names, OutlineIssue};

/// 一次「正文里冒出来的名字」扫出来的东西。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UnlistedReport {
    /// 候选（按出现次数从多到少）
    pub issues: Vec<OutlineIssue>,
    /// 一屏摆不下、没列出来的条数（**如实报**：截了就说，不静默丢）
    pub hidden: usize,
    /// 这次扫了几章（正文单位；作者好知道"扫到哪儿了"）
    pub scanned: usize,
}

impl Store {
    /// 全书扫一遍正文，认名字，跟设定卡比对（**只读**）。
    pub fn unlisted_names(&self, work_id: i64) -> Result<UnlistedReport> {
        let chapters = self.text_spine(work_id)?;
        // 先一次读全正文再抽：抽取要跨章统计"出现过几次"，逐章边读边算会多一次汇总
        let mut bodies: Vec<(i64, String)> = Vec::with_capacity(chapters.len());
        for chapter in &chapters {
            bodies.push((chapter.id, self.read_body(chapter.id)?));
        }
        let refs: Vec<(i64, &str)> = bodies.iter().map(|(id, body)| (*id, body.as_str())).collect();
        let cards = self.entity_cards(work_id, None)?;
        let (issues, hidden) = unlisted_name_issues(&unlisted_names(&refs, &cards));
        Ok(UnlistedReport {
            issues,
            hidden,
            scanned: chapters.len(),
        })
    }
}
