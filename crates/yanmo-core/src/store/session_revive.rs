//! 界面**卡死与复活**那本账：这一次会话里刚卡过没有、哪一刀动的手。
//!
//! 与 [`super::session`]（会话标记本身：该编辑哪一章 / 上次是不是正常退出）分开的原因：
//! 那一格管"进程活着还是死了"，这一格管"**界面**在不在"——守护之心那条线。
//! 两者的读者也不同：会话标记给崩溃恢复看，这一本给"重载回来要不要给作者一句交代"看。
//!
//! 两样东西住在同一个 `settings` 键上（`session.last` 那个标记），所以这里只读它、
//! 只往里补 `revive` 那一栏——标记本身的形状与读写仍在 [`super::session`] 里。

use super::session::UiFreeze;
use super::Store;
use crate::error::Result;

impl Store {
    /// **这一次会话里刚卡过没有**（守护之心那句交代的权威源）：只读标记里的 `revive`。
    ///
    /// 与 [`Store::peek_session`] 的分别：那个连"上次退得干不干净"一起答（给命令行与体检看），
    /// 这一条只回答"界面上要不要给作者一句交代"。
    /// 为什么不能拿 `begin_session` 那份快照：**它是启动那一刻的**——而卡死与复活发生在
    /// 这一次会话的中途（界面重载回来时会再问一遍），快照里永远是空的（真机演练逮到的）。
    pub fn session_revive(&self) -> Result<Option<UiFreeze>> {
        Ok(Self::read_marker(&self.conn)?.and_then(|marker| marker.revive))
    }

    /// **这一刀用了哪根杠杆、结果如何**：只追加一条 op-log（排查用）。
    ///
    /// 为什么不塞进会话标记：标记回答的是"界面卡过没有"（给作者那句交代用，形状要稳）；
    /// "哪一刀动了手、成没成"是**事后排查**要的，append-only 的 op-log 正合适——
    /// 真机演练里正是缺了这一条，才看不出第二刀到底动没动手。
    pub fn note_ui_revive(&mut self, attempt: u32, lever: &str, detail: &str, alive: bool) -> Result<()> {
        let node_id = Self::read_marker(&self.conn)?.and_then(|marker| marker.node_id);
        let tx = self.conn.transaction()?;
        Self::record_in(
            &self.device_id,
            &tx,
            "ui",
            node_id.unwrap_or(0),
            "revive",
            serde_json::json!({
                "attempt": attempt,
                "lever": lever,
                "detail": detail,
                "alive": alive,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }
}
