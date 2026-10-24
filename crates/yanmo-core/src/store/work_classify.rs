//! 作品的**分类归属**：体裁与标签（"分类维度"那一摊里人填的两格）的读与写。
//!
//! 为什么单独成件（与简介 / 总纲那两段话分开，也与 [`super::work`] 那张表的结构性字段分开）：
//! **它有一层别处没有的规矩——先归一再说**。体裁要去首尾空白，标签要按行读、去重、保序、
//! 超限当场拒（规则本身在 `model::tags`，这里只负责"写之前先过一遍、写完回读"）。
//! 简介与总纲要的是"存作者的原话、一个字都别动"，正好相反——两件事放在一个文件里，
//! 迟早有人照着这一半的规矩去改那一半。
//!
//! （0.102.0 从 `work_text.rs` 拆出来：那一次它同时还在长，`work.rs` 也越过了上帝线；
//! **拆的是变化理由，不是行数**——"归一过的元数据"与"原封不动的长文"本来就不是一回事。）

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{join_tags, normalize_genre, normalize_tags, parse_tags, WorkClassify};
use crate::time::now_millis;

impl Store {
    /// 读一本书的分类归属（体裁 + 标签）；书不在就是 `work.gone`。
    pub fn work_classify(&self, id: i64) -> Result<WorkClassify> {
        let (genre, tags): (String, String) = self
            .conn
            .query_row(
                "SELECT genre, tags FROM works WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]))?;
        Ok(WorkClassify { genre, tags })
    }

    /// 写分类归属（体裁 + 标签），返回**归一过、库里真有的那一份**。
    ///
    /// 与简介/总纲不同的一条：这两格**先归一再说**（去首尾空白 / 去重 / 去空行，
    /// 规则在 `model::tags`）——它们是拿来分组排序的，带着尾随空格、
    /// 同一个标签写两遍，都会让"主题"那一档排得没有道理。超限**如实拒**，不悄悄截断。
    ///
    /// 空串是合法的：**没填过**（既不写 NULL，也不报错）。
    pub fn set_work_classify(&mut self, id: i64, genre: &str, tags: &str) -> Result<WorkClassify> {
        let genre = normalize_genre(genre)?;
        let tags = join_tags(&normalize_tags(tags)?);

        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "UPDATE works SET genre = ?1, tags = ?2, updated_at = ?3
              WHERE id = ?4 AND deleted_at IS NULL",
            params![genre, tags, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        // 留痕记**归一之后**的标签个数（作者写的第 13 个会在上面那一步被拒）——
        // 与简介那条同一条纪律：日志不把整栏抄进去
        Self::record_in(
            &self.device_id,
            &tx,
            "works",
            id,
            "set_classify",
            json!({ "genre": genre, "tags": parse_tags(&tags).len() }),
        )?;
        tx.commit()?;
        self.work_classify(id)
    }
}
