//! 「对不上的地方」读一遍：把库里的设定卡与场景卡拉出来，交给纯逻辑那一层。
//!
//! 这里**只读不写**（扫描不该有任何副作用——作者点一次"看一眼"不该动一个字节），
//! 所以也没有留痕：真正的写是"忽略这一条"（见 [`super::outline_dismiss`]）。
//!
//! 分工：规则在 [`crate::outline::rules`]（纯值、可单测），这里只负责
//! "把值读齐"——读数据的形状（SQL）只有这一处知道。

use super::fragment::{into_fragment, read_raw, COLS};
use super::Store;
use crate::error::Result;
use crate::model::{Fragment, FragmentKind};
use crate::outline::{scan, OutlineData, OutlineIssue};

impl Store {
    /// 一本书里所有对不上的地方（**只读**）。
    ///
    /// 两次调用、同一份数据 → 同一个结果（顺序稳定）：界面上的清单不该自己跳。
    pub fn outline_issues(&self, work_id: i64) -> Result<Vec<OutlineIssue>> {
        let cards = self.entity_cards(work_id, None)?;
        let scenes = self.scene_cards_of_work(work_id)?;
        // 章的阅读顺序：伏笔"隔了多少章"与事件"在书里的位置"都靠它
        let chapters: Vec<i64> = self.text_spine(work_id)?.into_iter().map(|c| c.id).collect();
        let foreshadows = self.foreshadows(work_id, None)?;
        let events = self.dated_events(work_id)?;
        Ok(scan(&OutlineData {
            cards: &cards,
            scenes: &scenes,
            chapters: &chapters,
            foreshadows: &foreshadows,
            events: &events,
        }))
    }

    /// 这本书里**填了故事时间的事件**（没有上限）。
    ///
    /// 为什么不复用面板那个列表：那个带 200 条上限（一屏摆不下更多）——
    /// 拿它算顺序，超出的那些就静默不参与检查了。**少报比不报更难查**，
    /// 所以这里单走一条：只取真要算的那几条（`story_order` 非空）。
    fn dated_events(&self, work_id: i64) -> Result<Vec<Fragment>> {
        let sql = format!(
            "SELECT {COLS} FROM fragments
              WHERE work_id = ?1 AND frag_kind = ?2 AND deleted_at IS NULL
                AND story_order IS NOT NULL
              ORDER BY created_at, id"
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![work_id, FragmentKind::Event.as_str()],
            read_raw,
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(into_fragment(row?)?);
        }
        Ok(out)
    }
}
