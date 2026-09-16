//! 迁移 v11：**大纲本体的两样承载**——设定卡（人物 / 设定）与场景卡的四格。
//!
//! ⚠️ 与 v1~v10 同样**一旦发布不可再改**。
//!
//! # 为什么是两张新表，而不是往既有表上挂
//!
//! 1. **设定卡不是碎片**：碎片（`fragments`）是**素材**——一句话，会被"用掉"、
//!    会随时间冷却降权；设定卡是**实体**——有稳定身份（名字 / 别称）与键值属性，
//!    不该因为"用过一次"就降权。混进同一张表，素材那套引力语义（`used_count` /
//!    `importance`）就会污染设定（理由全文见 `model::entity_card`）。
//! 2. **场景卡的四格不进 `nodes`**：目录树那张表是**所有节点**共用的，读树时不该拖着
//!    只有场景卡才有的四列；这与 v1 把 `node_contents`（正文）从 `nodes`（结构）里
//!    拆出去是同一条理由——**各自的变化理由不一样，就各住各的表**。
//!    卫星表还顺手给了个好处：没有这一行 = "这张卡还没填过"，四格全空是它的自然语义。
//!
//! # 列与语义
//!
//! - `entity_cards.card_kind`：`person` / `setting`（稳定码，见 `model::EntityKind`）；
//! - `entity_cards.aliases` / `attributes`：**JSON 文本**（前者是字符串数组，后者是
//!   `[{"key":…,"value":…}]`）。存 JSON 而不是再开两张子表：它们小、有序、只属于这张卡，
//!   子表只会多一个写入口；而且**认不出来就报错**（要拿去做冲突检测的列，不许静默少一截）；
//! - `entity_cards.name`：正式名——**称谓冲突认它**（两张卡不许共用同一个名字）；
//! - `deleted_at`：软删（与作品、章节、碎片同一条纪律：删只是打时间戳）。
//!
//! 外键：都挂在 `works` 上并级联删（书真被清掉时，卡没有留着的理由）；
//! 场景卡的四格挂在 `nodes` 上并级联删（节点没了，四格无主）。
//!
//! 索引：`(work_id, card_kind, deleted_at)` 与 `(work_id, deleted_at)` 都按最常用的那种
//! 问法（"这本书的人物有哪些" / "这本书的设定卡全列出来"）覆盖。

/// v11 的静态步骤：建两张表 + 建索引（都幂等，不需要 `prepare`）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS entity_cards (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        work_id    INTEGER NOT NULL REFERENCES works(id) ON DELETE CASCADE,
        card_kind  TEXT    NOT NULL,
        name       TEXT    NOT NULL,
        aliases    TEXT    NOT NULL DEFAULT '[]',
        attributes TEXT    NOT NULL DEFAULT '[]',
        note       TEXT    NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        deleted_at INTEGER
    )",
    "CREATE INDEX IF NOT EXISTS idx_entity_cards_work
        ON entity_cards(work_id, card_kind, deleted_at)",
    "CREATE TABLE IF NOT EXISTS scene_cards (
        node_id    INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
        pov        TEXT    NOT NULL DEFAULT '',
        goal       TEXT    NOT NULL DEFAULT '',
        conflict   TEXT    NOT NULL DEFAULT '',
        outcome    TEXT    NOT NULL DEFAULT '',
        updated_at INTEGER NOT NULL
    )",
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
