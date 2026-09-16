//! 作品的两段话：**简介**（给别人看的）与**故事总纲**（给自己看的）。
//!
//! 为什么单独成件：它们与"作品本身"（标题 / 类型 / 语言 / 书架规模）的变化理由不一样——
//! 这两段都是**长自由文本**，共用同一条规矩：存作者的原话（不 trim、不改标点、不动换行）、
//! 留痕只记字数、回执取库里那一份。而 [`super::work`] 管的是那张表的结构性字段。
//!
//! （0.68.0 加故事总纲时 `work.rs` 越过 400 行，顺手把这两段拆出来——
//! **拆的是变化理由，不是行数**：一段长文怎么存、怎么改，与本作品叫什么名字无关。）

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

impl Store {
    /// 写作品简介（投稿包的大纲要用它）。
    ///
    /// 同"每章一句话"一条规矩：**存作者的原话**，不 trim、不改标点；
    /// 日志只记字数，不把整段话抄进变更留痕。
    pub fn set_work_summary(&mut self, id: i64, summary: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "UPDATE works SET summary = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![summary, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "works",
            id,
            "set_summary",
            json!({ "chars": summary.chars().count() }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 读一本书的故事总纲（**作者一个字都没写过就是空串**）。
    ///
    /// 单独一个读入口而不是把 `storyline` 挂在书架那一份上：书架列表为了排序会一次读全表，
    /// 总纲是一段可能很长的自由文本——**书架不该拖着它**（与目录树不带正文同一条纪律）。
    pub fn work_storyline(&self, id: i64) -> Result<String> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT storyline FROM works WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        text.ok_or_else(|| Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]))
    }

    /// 写故事总纲，返回**库里真有的那一份**。
    ///
    /// 与作品简介同一条规矩：**存作者的原话**（不 trim、不改标点、不动换行），
    /// 日志只记字数；空串就是"没写过"（不区分"删光了"与"从没写过"——对作者是同一件事）。
    pub fn set_work_storyline(&mut self, id: i64, storyline: &str) -> Result<String> {
        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "UPDATE works SET storyline = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![storyline, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "works",
            id,
            "set_storyline",
            json!({ "chars": storyline.chars().count() }),
        )?;
        tx.commit()?;
        self.work_storyline(id)
    }
}
