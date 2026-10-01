//! 迁移 v19：**五线推进度**——一张 `node_lines` 关联表（章 → 它推进的故事线）。
//!
//! ⚠️ 与 v1~v18 同样**一旦发布不可再改**。
//!
//! # 这张表为什么存在
//!
//! 「伏笔线 / 碎片线 / 人物线」三条能从作者已经在写的东西里**派生**出来（0.92.0 / 0.93.0），
//! 而「主线 / 副线 / 感情线 / 悬念线 / 成长线」这五条**派生不出来**：它们是作者对
//! "这一章在推进哪几条线"的判断，软件替作者猜不了（猜了就是替作者写大纲）。所以要有一处
//! 落得住这个判断的**标注**。
//!
//! # 为什么是一张关联表，不是 `nodes` 上一个 JSON / 逗号串列
//!
//! 与 v13 的 `node_cast` 同一条理由：它有两个问法（"这一章在推哪几条线"与"这条线推进到哪几章"），
//! 而且第二问将来一定会有人问（"感情线断了多少章"）。塞成一列字符串，第二问就只能把整棵树的
//! 那一列全解一遍；也不会有"同一章同一条线只算一次"这条由**主键**保证的幂等。
//!
//! # 列与语义
//!
//! - 主键 `(node_id, line)`：同一章同一条线只算一次（重复插入是幂等，不是报错）；
//! - `line`：`StoryLine` 的**稳定码**（`main` / `sub` / `romance` / `suspense` / `growth`）。
//!   列里不写中文名——中文名住在界面字典里（核心零文案）；
//!   列是 TEXT 而不是 CHECK/enum：加第六条线时不该动表结构（码的闭集由核心那张表管，
//!   认不出的码读的时候当场报错，见 `model::StoryLine::parse`）；
//! - `created_at`：这一笔是**什么时候标上的**（与 `node_cast` 同一个口径）；
//! - 索引 `idx_node_lines_line`：反向那一问（"这条线推进到哪几章"）的入口，与
//!   `idx_node_cast_entity` 同一个理由——先备着，问那一刻不用改表；
//! - 外键挂 `nodes(id)` 并级联删（段真被清掉时，标注没有留着的理由）；
//! - **不存 `work_id`**：它由 `node_id` 派生（`nodes.work_id`），存第二份就会有不一致的那天。
//!
//! **没标 ≠ 没推进**：这张表里一行都没有的章，意思是"作者还没标"，
//! 不是"这一章一条线都没推进"——界面不许替作者补这一句。

/// v19 的静态步骤：建表 + 建反向索引（都幂等，不需要 `prepare`）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS node_lines (
        node_id    INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        line       TEXT    NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (node_id, line)
    )",
    "CREATE INDEX IF NOT EXISTS idx_node_lines_line ON node_lines(line)",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_is_a_single_statement() {
        for (i, s) in STEPS.iter().enumerate() {
            let t = s.trim();
            assert!(t.ends_with(')') || !t.contains(';'), "步骤 {i} 可能含多条语句：{t}");
            assert!(!t.contains(';'), "步骤 {i} 含分号（多语句）——迁移必须逐条 execute：{t}");
        }
    }

    /// 步骤都是幂等的：连跑两遍不该炸（`CREATE ... IF NOT EXISTS` 的用意就在这里）。
    #[test]
    fn steps_are_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE nodes(id INTEGER PRIMARY KEY)", []).unwrap();
        for _ in 0..2 {
            for step in STEPS {
                conn.execute(step, []).unwrap();
            }
        }
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM node_lines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }
}
