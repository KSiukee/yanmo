//! 「对不上的地方」读一遍：把库里的设定卡与场景卡拉出来，交给纯逻辑那一层。
//!
//! 这里**只读不写**（扫描不该有任何副作用——作者点一次"看一眼"不该动一个字节），
//! 所以也没有留痕：真正的写是"忽略这一条"（见 [`super::outline_dismiss`]）。
//!
//! 分工：规则在 [`crate::outline::rules`]（纯值、可单测），这里只负责
//! "把值读齐"——读数据的形状（SQL）只有这一处知道。

use super::Store;
use crate::error::Result;
use crate::outline::{scan, OutlineData, OutlineIssue};

impl Store {
    /// 一本书里所有对不上的地方（**只读**）。
    ///
    /// 两次调用、同一份数据 → 同一个结果（顺序稳定）：界面上的清单不该自己跳。
    pub fn outline_issues(&self, work_id: i64) -> Result<Vec<OutlineIssue>> {
        let cards = self.entity_cards(work_id, None)?;
        let scenes = self.scene_cards_of_work(work_id)?;
        Ok(scan(&OutlineData {
            cards: &cards,
            scenes: &scenes,
        }))
    }
}
