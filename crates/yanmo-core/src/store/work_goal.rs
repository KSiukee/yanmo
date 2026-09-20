//! 作品的**目标字数**：这本书打算写多少字（可空＝没设目标）。
//!
//! 单独成件而不是塞进 `work.rs`：它与"作品叫什么名字、什么类型"的变化理由不一样——
//! 那几样一改就是另一本书，这一格只是作者给自己定的一根线，随时可以挪、可以取消。
//!
//! 短篇的「未完成散篇」视图正是拿它与当前字数对出来的（**开了坑没填**是短篇的主要浪费源，
//! 判据就是"目标字数 vs 当前字数 + 最后编辑时间"）。这里只管这一格怎么存、怎么留痕；
//! "算不算未完成"由界面拿它与 `ManuscriptRow::char_count` 比，核心不替作者下结论。
//!
//! ⚠️ 这一格自 v1 就在表里，但一直**没有写入口**（读得到、写不进）——补上它是为了让
//! 「未完成散篇」的判据有料可用，而不是让界面自己去写库。

use rusqlite::params;
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

impl Store {
    /// 这本书的目标字数（没设过＝`None`）。
    pub fn work_target_words(&self, id: i64) -> Result<Option<i64>> {
        Ok(self.get_work(id)?.target_words)
    }

    /// 设定 / 清除目标字数：`None`（或 ≤0）= 清掉，回到"没设过"。
    ///
    /// 与每卷目标章数（`set_volume_target`）同一条做法：这是作者自己定的参考线，
    /// **不进任何达标判据**，所以只做最朴素的校验——非正数一律当"清掉"。
    pub fn set_work_target_words(&mut self, id: i64, words: Option<i64>) -> Result<()> {
        let target = words.filter(|n| *n > 0);
        let tx = self.conn.transaction()?;
        let affected = tx.execute(
            "UPDATE works SET target_words = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![target, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "works",
            id,
            "set_target_words",
            json!({ "words": target }),
        )?;
        tx.commit()?;
        Ok(())
    }
}
