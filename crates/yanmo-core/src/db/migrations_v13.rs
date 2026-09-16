//! 迁移 v13：**出场人物**——节点（章 / 节 / 场景卡）↔ 设定卡的关联。
//!
//! ⚠️ 与 v1~v12 同样**一旦发布不可再改**。
//!
//! # 为什么是一张关联表，不是 `nodes` 上一个 JSON 列
//!
//! 1. **它有两个主人**：一条关联同时属于"哪一章"与"哪个人物"。塞进 `nodes` 的 JSON 列里，
//!    "这个人物在哪几章出场"就必须把整棵树的 JSON 全解一遍；反过来也一样。
//!    两张小表（`node_cast` 的两个方向）才是它能被问得动的形状；
//! 2. **改名不用动它**：关联认的是 `entity_cards.id`。作者把「陆文」改成「陆文昭」，
//!    关联一个字都不用改——名字是卡的事，不是关联的事（这也是它不复用 `fragments` 的理由：
//!    碎片是素材、会被用掉、会冷却，出场人物是**结构**）；
//! 3. **删了卡不留脏**：卡是软删（`deleted_at`），所以外键级联不会响——读的时候一律
//!    JOIN `entity_cards ... deleted_at IS NULL`，软删的卡自动从每一章的人名里消失；
//!    卡要是真被清掉（级联删书那一类），关联跟着走。
//!
//! # 列与语义
//!
//! - 主键 `(node_id, entity_id)`：同一个人在同一章里只算一次（重复插入是幂等，不是报错）；
//! - `created_at`：这一笔是**什么时候挂上去的**（作者问"这条线是什么时候想起来的"时有用）；
//! - 索引 `idx_node_cast_entity`：反向那一问（"这个人物在哪几章出现过"）已经有人会问它了——
//!    0.66.0 的大纲表只读正向（一章有谁），但反向是这条路走两步就到的地方，索引先备着；
//! - 不存 `work_id`：它由 `node_id` 派生（`nodes.work_id`），存第二份就会有不一致的那天。

/// v13 的静态步骤：建表 + 建反向索引（都幂等，不需要 `prepare`）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS node_cast (
        node_id    INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        entity_id  INTEGER NOT NULL REFERENCES entity_cards(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (node_id, entity_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_node_cast_entity ON node_cast(entity_id)",
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
}
