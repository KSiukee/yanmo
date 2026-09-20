//! 迁移 v18：**短篇集**——一张 `collection_items` 引用表（集 → 篇 + 排序）。
//!
//! ⚠️ 与 v1~v17 同样**一旦发布不可再改**。
//!
//! # 为什么是"引用"而不是"把文章放进集里"
//!
//! 一篇稿子**只写一次**，但可以同时属于多个集（年度自选集 / 科幻集 / 投稿用集）——
//! 这是真实用法。若把集做成"装着篇的容器"（集是 Work、篇是它的节点），归集就成了**移动**，
//! 一篇只能待在一个地方，而且没归集的散篇得塞进一个叫"散篇"的垃圾桶作品里。
//! 用引用表，两件事各自成立：**文章仍是一等作品**（书架、稿件台照旧管它），
//! 集只是"挑出来的那一串"。
//!
//! # 列与语义
//!
//! - `collection_id` / `article_id`：都挂在 `works` 上并级联删（书真被清掉时，引用没有留着的理由）；
//! - `sort_order`：集内顺序（作者可拖）。**默认追加在末尾**（取当前最大 +1），
//!   不写死"按时间"——排序是作者的事，软件只保证它稳定；
//! - `(collection_id, article_id)` 唯一：同一篇在同一集里不会出现两次（加两次是幂等的）；
//! - 本篇被软删（进回收站）时**引用不删**：捞回来还在原来的位置上——
//!   软删的纪律是"删只是打时间戳"，引用跟着一起消失就把作者排好的顺序弄丢了。
//!
//! 索引按最常用的两种问法覆盖："这个集里有哪些篇"（`collection_id, sort_order`）
//! 与"这篇属于哪几个集"（`article_id`）。

/// v18 的静态步骤：建表 + 三个索引（都幂等，不需要 `prepare`）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS collection_items (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        collection_id INTEGER NOT NULL REFERENCES works(id) ON DELETE CASCADE,
        article_id    INTEGER NOT NULL REFERENCES works(id) ON DELETE CASCADE,
        sort_order    INTEGER NOT NULL DEFAULT 0,
        created_at    INTEGER NOT NULL
    )",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_collection_items_once
        ON collection_items(collection_id, article_id)",
    "CREATE INDEX IF NOT EXISTS idx_collection_items_order
        ON collection_items(collection_id, sort_order)",
    "CREATE INDEX IF NOT EXISTS idx_collection_items_article
        ON collection_items(article_id)",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 步骤都是幂等的：连跑两遍不该炸（`CREATE ... IF NOT EXISTS` 的用意就在这里）。
    #[test]
    fn steps_are_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE works(id INTEGER PRIMARY KEY)", []).unwrap();
        for _ in 0..2 {
            for step in STEPS {
                conn.execute(step, []).unwrap();
            }
        }
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM collection_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }
}
