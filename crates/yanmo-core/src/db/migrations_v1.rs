//! 迁移 v1：初始 schema。
//!
//! ⚠️ **本文件一旦发布就不可再改**（迁移铁律 1）。要改结构，请在 `migrations.rs`
//! 追加 v2。
//!
//! # 设计约束
//!
//! - **可变深度节点树**：`nodes` 靠 `parent_id` 自引用，"卷/章"只是
//!   `node_kind` 的取值——**不得**把卷/章做成固定两级表。
//! - **作品类型是一等公民**：`works.kind` 区分长篇/文章/短篇集。
//! - **正文与结构分表**：目录树只读 `nodes`，正文按需拉。
//! - **op-log 变更日志**：append-only + 设备标识 + 本机单调序号，
//!   为多端同步预留。
//! - **碎片统一存储**：事件/灵感速记/口述段落/答案池/问题卡同表。
//! - **软删除**：`deleted_at` 而非真删（回收站）。

/// v1 的建表语句（**每条一个完整语句**，由迁移框架在事务内逐条执行）。
pub const STEPS: &[&str] = &[
    // ── 作品 ──────────────────────────────────────────────────────────
    "CREATE TABLE works (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        kind         TEXT    NOT NULL,
        title        TEXT    NOT NULL,
        target_words INTEGER,
        cover        TEXT    NOT NULL DEFAULT '',
        created_at   INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL,
        opened_at    INTEGER,
        archived_at  INTEGER,
        deleted_at   INTEGER,
        sort_order   INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE INDEX idx_works_opened ON works(opened_at DESC)",
    "CREATE INDEX idx_works_deleted ON works(deleted_at)",
    // ── 结构节点（可变深度树）─────────────────────────────────────────
    "CREATE TABLE nodes (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        work_id    INTEGER NOT NULL REFERENCES works(id) ON DELETE CASCADE,
        parent_id  INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
        node_kind  TEXT    NOT NULL,
        title      TEXT    NOT NULL DEFAULT '',
        sort_order INTEGER NOT NULL DEFAULT 0,
        word_count INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        deleted_at INTEGER
    )",
    "CREATE INDEX idx_nodes_tree ON nodes(work_id, parent_id, sort_order)",
    "CREATE INDEX idx_nodes_deleted ON nodes(deleted_at)",
    // ── 正文（与结构分表，懒加载）─────────────────────────────────────
    "CREATE TABLE node_contents (
        node_id      INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
        body         TEXT    NOT NULL DEFAULT '',
        content_hash TEXT    NOT NULL DEFAULT '',
        char_count   INTEGER NOT NULL DEFAULT 0,
        updated_at   INTEGER NOT NULL
    )",
    // ── 每章版本快照（恢复操作前也先备份一次）────────────────────
    "CREATE TABLE snapshots (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        node_id    INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
        body       TEXT    NOT NULL,
        char_count INTEGER NOT NULL DEFAULT 0,
        reason     TEXT    NOT NULL DEFAULT 'auto',
        created_at INTEGER NOT NULL
    )",
    "CREATE INDEX idx_snapshots_node ON snapshots(node_id, created_at DESC)",
    // ── op-log：append-only 变更日志（多端同步基石）───────────────────
    "CREATE TABLE op_log (
        seq       INTEGER PRIMARY KEY AUTOINCREMENT,
        device_id TEXT    NOT NULL,
        entity    TEXT    NOT NULL,
        entity_id INTEGER NOT NULL,
        op        TEXT    NOT NULL,
        payload   TEXT    NOT NULL DEFAULT '{}',
        created_at INTEGER NOT NULL
    )",
    "CREATE INDEX idx_oplog_entity ON op_log(entity, entity_id)",
    // ── 设备（多端同步）──────────────────────────────────────────────
    "CREATE TABLE devices (
        id         TEXT    PRIMARY KEY,
        name       TEXT    NOT NULL DEFAULT '',
        last_seq   INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
    )",
    // ── 碎片统一存储（事件/灵感/口述/答案池/问题卡）───────────────────
    "CREATE TABLE fragments (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        work_id      INTEGER REFERENCES works(id) ON DELETE CASCADE,
        frag_kind    TEXT    NOT NULL,
        body         TEXT    NOT NULL DEFAULT '',
        source       TEXT    NOT NULL DEFAULT 'typed',
        status       TEXT    NOT NULL DEFAULT 'pending',
        importance   REAL    NOT NULL DEFAULT 0.5,
        used_count   INTEGER NOT NULL DEFAULT 0,
        linked       TEXT    NOT NULL DEFAULT '[]',
        derived_from INTEGER REFERENCES fragments(id) ON DELETE SET NULL,
        created_at   INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL,
        deleted_at   INTEGER
    )",
    "CREATE INDEX idx_fragments_work ON fragments(work_id, frag_kind, deleted_at)",
    "CREATE INDEX idx_fragments_recent ON fragments(created_at DESC)",
    // ── 键值设置 ─────────────────────────────────────────────────────
    "CREATE TABLE settings (
        key        TEXT    PRIMARY KEY,
        value      TEXT    NOT NULL,
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
            assert!(t.ends_with(')') || t.ends_with("')") || !t.contains(';'), "步骤 {i} 可能含多条语句：{t}");
            assert!(!t.contains(';'), "步骤 {i} 含分号（多语句）——迁移必须逐条 execute：{t}");
        }
    }

    #[test]
    fn no_volume_or_chapter_as_tables() {
        // ★ 铁律：卷/章只能是 node_kind 的取值，不得成为表名
        for s in STEPS {
            let lower = s.to_lowercase();
            assert!(!lower.contains("create table volumes"), "禁止把『卷』做成固定表");
            assert!(!lower.contains("create table chapters"), "禁止把『章』做成固定表");
        }
    }
}
