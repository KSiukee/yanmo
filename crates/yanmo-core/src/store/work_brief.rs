//! 一本书的**一眼信息**：类型、语言、**落定后的字数口径**——外加记一次"打开"。
//!
//! 与 [`Store::work_target`](crate::store::Store::work_target) 分开：那条是"要开编辑器，
//! 给我一个可写落点"（没有能落笔的节点就**补一章**）；这条是"只是想切过去"。
//!
//! # 为什么要分开
//!
//! **集没有正文**。打开一个集若走"给我一个可写落点"那条路，核心会好心给集补一章——
//! 于是每打开一次集子就凭空长出一个没人要的正文节点；而作者眼里的集是**引用清单**，
//! 根本没有"这一集写什么"这件事。所以界面切作品时先问这一眼，
//! 是集就只切"当前作品"、主区换成集视图。
//!
//! 顺手记一次"打开"（书架排序看 `opened_at`）：从书架、稿件台还是集视图切过去都算打开。

use super::Store;
use crate::error::Result;
use crate::model::Work;
use crate::text::WordCaliber;

/// 切作品之前要的那一眼。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkBrief {
    pub work: Work,
    /// **落定后**的字数口径（作者选过就是它，没选过就是作品语言的默认）
    pub caliber: WordCaliber,
}

impl Store {
    /// 一本书的一眼信息（顺手记一次打开）。书不在（或进了回收站）会明确报错。
    pub fn work_brief(&mut self, work_id: i64) -> Result<WorkBrief> {
        super::work::ensure_alive(&self.conn, work_id)?;
        let work = self.get_work(work_id)?;
        let caliber = self.word_caliber(work_id)?;
        self.touch_work_opened(work_id)?;
        Ok(WorkBrief { work, caliber })
    }
}
