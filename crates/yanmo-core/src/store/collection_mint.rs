//! 集**自己**的那两件事：建一个集、给定集目标。
//!
//! 与 [`super::collection`]（集里的成员与顺序）分开的理由：变化理由不一样——
//! 那边围着"引用"转（加/拿/排，一篇可以同时在几个集里），这边围着"集这件作品"转
//! （它叫什么、想收多少篇多少字）。混在一起，两边的方法会互相挤着长。
//!
//! # 建集的两条纪律
//!
//! - **复用 [`Store::create_work`]**：集也是一件作品（根节点、留痕、软删都照旧），
//!   不另开一套"建集"的动作；
//! - **装篇在一个事务里一次装齐**：装失败时报错如实（集还在、只是空的）——
//!   比"装了一半还不吭声"好：作者再点一次「加入」就能补齐。
//!
//! # 集目标是什么
//!
//! 篇数与总字数（`work.{id}.collection_pieces` / `..._words`，与每卷目标章数同一套做法）。
//! 它只是**作者给自己定的一根线**（够 N 篇就该编成集了、这本集子想出到多少字），
//! **不进任何达标判据**——所以只做最朴素的校验，≤0 一律当"清掉"。

use rusqlite::params;
use serde_json::json;

use super::collection::ensure_article;
use super::Store;
use crate::error::Result;
use crate::model::{Work, WorkKind};
use crate::time::now_millis;

impl Store {
    /// 一键建集：建一个集，把这几个篇目**按给定顺序**装进去（攒集提示那个按钮走这条）。
    pub fn create_collection_from(&mut self, title: &str, article_ids: &[i64]) -> Result<Work> {
        let work = self.create_work(WorkKind::Collection, title)?;
        for article_id in article_ids {
            ensure_article(&self.conn, *article_id)?;
        }
        let now = now_millis();
        let tx = self.conn.transaction()?;
        for (index, article_id) in article_ids.iter().enumerate() {
            tx.execute(
                "INSERT OR IGNORE INTO collection_items
                        (collection_id, article_id, sort_order, created_at)
                 VALUES(?1, ?2, ?3, ?4)",
                params![work.id, article_id, index as i64, now],
            )?;
        }
        Store::record_in(
            &self.device_id,
            &tx,
            "works",
            work.id,
            "collect_many",
            json!({ "article_ids": article_ids }),
        )?;
        tx.commit()?;
        Ok(work)
    }

    /// 定集目标：篇数与总字数（`None` 或 ≤0 = 清掉那一项，回到"没设过"）。
    pub fn set_collection_target(
        &mut self,
        collection_id: i64,
        pieces: Option<i64>,
        words: Option<i64>,
    ) -> Result<()> {
        // 只有活着的集谈得上目标（借用成员那边的守卫，口径只留一处）
        super::collection::ensure_collection(&self.conn, collection_id)?;
        let pieces = pieces.filter(|n| *n > 0);
        let words = words.filter(|n| *n > 0);
        let tx = self.conn.transaction()?;
        write_target(&tx, &target_key(collection_id, "pieces"), pieces)?;
        write_target(&tx, &target_key(collection_id, "words"), words)?;
        Store::record_in(
            &self.device_id,
            &tx,
            "works",
            collection_id,
            "set_collection_target",
            json!({ "pieces": pieces, "words": words }),
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// 集目标在 `settings` 里的键（按集分开，互不干扰）。
pub(super) fn target_key(collection_id: i64, which: &str) -> String {
    format!("work.{collection_id}.collection_{which}")
}

fn write_target(tx: &rusqlite::Transaction<'_>, key: &str, value: Option<i64>) -> Result<()> {
    match value {
        Some(n) => {
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
                params![key, n.to_string(), now_millis()],
            )?;
        }
        None => {
            tx.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        }
    }
    Ok(())
}
