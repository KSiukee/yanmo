//! 短篇集的**写入口**：往集里加一篇、拿掉一篇、重排、定集目标。
//!
//! 集是**引用聚合**（`collection_items`：集 → 篇 + 排序），不是一个装着篇的容器：
//! 所以这里只动引用表，**从不碰正文**——一篇稿子仍然是一等作品，书架与稿件台照旧管它。
//!
//! # 留痕
//!
//! 每次改动都写一条 op-log，`entity='works'`、`entity_id=` **集**的 id：
//! 集自己也是一件作品，它在留痕里就该像一件作品那样出现。
//!
//! # 分寸
//!
//! - **加两次是幂等的**（`(集, 篇)` 唯一）：界面从清单里挑，手抖点两次不该报错；
//! - **拿掉不删正文**：只是把引用去掉（正文还在书架 / 稿件台上）；
//! - **重排拒绝"对不上"的清单**：给的名单必须与集里现有的篇目**正好一样**，
//!   少一篇多一篇都当场拒绝、一个字节都不写（那多半是界面拿着过期的清单来写）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

impl Store {
    /// 往集里加一篇（**追加在末尾**）。已经在集里就是空操作，返回是否真的加了。
    pub fn add_to_collection(&mut self, collection_id: i64, article_id: i64) -> Result<bool> {
        ensure_collection(&self.conn, collection_id)?;
        ensure_article(&self.conn, article_id)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM collection_items WHERE collection_id = ?1",
            params![collection_id],
            |r| r.get(0),
        )?;
        let added = tx.execute(
            "INSERT OR IGNORE INTO collection_items(collection_id, article_id, sort_order, created_at)
             VALUES(?1, ?2, ?3, ?4)",
            params![collection_id, article_id, next, now],
        )?;
        if added > 0 {
            Store::record_in(
                &self.device_id,
                &tx,
                "works",
                collection_id,
                "collect",
                json!({ "article_id": article_id, "sort_order": next }),
            )?;
        }
        tx.commit()?;
        Ok(added > 0)
    }

    /// 从集里拿掉一篇（**只去掉引用，不碰正文**）。返回是否真的拿掉了。
    pub fn remove_from_collection(&mut self, collection_id: i64, article_id: i64) -> Result<bool> {
        ensure_collection(&self.conn, collection_id)?;
        let tx = self.conn.transaction()?;
        let removed = tx.execute(
            "DELETE FROM collection_items WHERE collection_id = ?1 AND article_id = ?2",
            params![collection_id, article_id],
        )?;
        if removed > 0 {
            Store::record_in(
                &self.device_id,
                &tx,
                "works",
                collection_id,
                "uncollect",
                json!({ "article_id": article_id }),
            )?;
        }
        tx.commit()?;
        Ok(removed > 0)
    }

    /// 按给定顺序重排集内篇目。
    ///
    /// 名单必须与集里现有的篇目**正好一样**（顺序随意）：少一篇多一篇都拒绝，
    /// 且**一个字节都不写**——界面拿着一份过期清单来写，宁可不改也不能把篇目弄丢。
    pub fn reorder_collection(&mut self, collection_id: i64, article_ids: &[i64]) -> Result<()> {
        ensure_collection(&self.conn, collection_id)?;
        let current = member_ids(&self.conn, collection_id)?;
        let same = {
            let mut wanted: Vec<i64> = article_ids.to_vec();
            wanted.sort_unstable();
            let given = wanted.len();
            wanted.dedup();
            // 既要是同一批（集合相等），也不能有重复项——名单里同一篇出现两次，
            // 说明界面那份清单是坏的，照它写会把顺序排成谁也没想要的样子
            wanted == current && given == current.len()
        };
        if !same {
            return Err(Error::invalid_with(
                codes::COLLECTION_REORDER_MISMATCH,
                [
                    ("expected", current.len().to_string()),
                    ("got", article_ids.len().to_string()),
                ],
            ));
        }
        let now = now_millis();
        let tx = self.conn.transaction()?;
        for (index, article_id) in article_ids.iter().enumerate() {
            tx.execute(
                "UPDATE collection_items SET sort_order = ?1
                  WHERE collection_id = ?2 AND article_id = ?3",
                params![index as i64, collection_id, article_id],
            )?;
        }
        Store::record_in(
            &self.device_id,
            &tx,
            "works",
            collection_id,
            "reorder",
            json!({ "article_ids": article_ids }),
        )?;
        // 集自己的 updated_at 跟着动（书架按它排序，改过集就该浮上来）
        tx.execute(
            "UPDATE works SET updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, collection_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 定集目标：篇数与总字数（`None` 或 ≤0 = 清掉那一项，回到"没设过"）。
    ///
    /// 目标只是**作者给自己定的一根线**（够 N 篇就能编成一个集了、或者这本集子想出到多少字），
    /// 不进任何达标判据——所以只做最朴素的校验。
    pub fn set_collection_target(
        &mut self,
        collection_id: i64,
        pieces: Option<i64>,
        words: Option<i64>,
    ) -> Result<()> {
        ensure_collection(&self.conn, collection_id)?;
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

/// 集目标在 `settings` 里的键（按集分开，互不干扰；与每卷目标章数同一套做法）。
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

/// 集内现有的篇目 id（升序；顺序在这里不重要，只用来对账）。
fn member_ids(conn: &rusqlite::Connection, collection_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT article_id FROM collection_items WHERE collection_id = ?1 ORDER BY article_id",
    )?;
    let rows = stmt.query_map(params![collection_id], |r| r.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 这真的是一个**活着的集**吗。
fn ensure_collection(conn: &rusqlite::Connection, work_id: i64) -> Result<()> {
    let kind: Option<String> = conn
        .query_row(
            "SELECT kind FROM works WHERE id = ?1 AND deleted_at IS NULL",
            params![work_id],
            |r| r.get(0),
        )
        .optional()?;
    match kind.as_deref() {
        Some("collection") => Ok(()),
        _ => Err(Error::invalid_with(
            codes::COLLECTION_NOT_A_COLLECTION,
            [("work_id", work_id.to_string())],
        )),
    }
}

/// 这真的是一篇**活着的单篇文章**吗（长篇 / 短篇集自己都不能被归集）。
fn ensure_article(conn: &rusqlite::Connection, work_id: i64) -> Result<()> {
    let kind: Option<String> = conn
        .query_row(
            "SELECT kind FROM works WHERE id = ?1 AND deleted_at IS NULL",
            params![work_id],
            |r| r.get(0),
        )
        .optional()?;
    match kind.as_deref() {
        Some("article") => Ok(()),
        _ => Err(Error::invalid_with(
            codes::COLLECTION_MEMBER_NOT_ARTICLE,
            [("work_id", work_id.to_string())],
        )),
    }
}
