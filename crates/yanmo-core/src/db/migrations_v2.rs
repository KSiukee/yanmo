//! 迁移 v2：全文检索索引（FTS5 + trigram 分词）。
//!
//! ⚠️ 与 v1 同样**一旦发布不可再改**。
//!
//! # 为什么是 trigram
//!
//! 中文没有词间空格，`unicode61` 会把一整句并成一个 token，检索等于没有。
//! `trigram` 把文本切成三字滑窗，因此**任意 ≥3 字的子串都能命中**——这正是"写作时找一句话"
//! 的真实用法（SQLite 3.34+ 才有，启动期已做真实功能探针）。
//!
//! # 为什么用普通 FTS 表（而不是 contentless / 外部内容）
//!
//! 索引表自己存一份文本，`UPDATE` / `DELETE` 才能正常工作，短查询（1–2 字）也还能用
//! `instr` 全扫兜底。代价是索引里多存一份正文——它是**索引不是权威**，权威始终是 `node_contents`。
//!
//! # 同步方式
//!
//! 由触发器维护，**不靠应用代码记得去更新索引**（漏一次就永久搜不到）。标题在 `nodes`，
//! 正文在 `node_contents`，两张表的写入都会同步到同一行 `node_fts`（rowid = 节点 id）。

/// v2 的语句（**每条一个完整语句**，由迁移框架在事务内逐条执行）。
///
/// 注意：触发器语句体内部含分号，这是 SQL 语法的一部分；每条仍是一个语句。
pub const STEPS: &[&str] = &[
    // 检索索引：rowid = nodes.id
    "CREATE VIRTUAL TABLE node_fts USING fts5(title, body, tokenize='trigram')",
    // 存量回填（新建库时是空操作）
    "INSERT INTO node_fts(rowid, title, body)
        SELECT n.id, n.title, COALESCE(c.body, '')
        FROM nodes n LEFT JOIN node_contents c ON c.node_id = n.id",
    // ── 结构（标题）同步 ─────────────────────────────────────────────
    "CREATE TRIGGER nodes_fts_ai AFTER INSERT ON nodes BEGIN
        INSERT INTO node_fts(rowid, title, body) VALUES (new.id, new.title, '');
     END",
    "CREATE TRIGGER nodes_fts_au AFTER UPDATE OF title ON nodes BEGIN
        UPDATE node_fts SET title = new.title WHERE rowid = new.id;
     END",
    "CREATE TRIGGER nodes_fts_ad AFTER DELETE ON nodes BEGIN
        DELETE FROM node_fts WHERE rowid = old.id;
     END",
    // ── 正文同步 ─────────────────────────────────────────────────────
    "CREATE TRIGGER contents_fts_ai AFTER INSERT ON node_contents BEGIN
        UPDATE node_fts SET body = new.body WHERE rowid = new.node_id;
     END",
    "CREATE TRIGGER contents_fts_au AFTER UPDATE OF body ON node_contents BEGIN
        UPDATE node_fts SET body = new.body WHERE rowid = new.node_id;
     END",
    "CREATE TRIGGER contents_fts_ad AFTER DELETE ON node_contents BEGIN
        UPDATE node_fts SET body = '' WHERE rowid = old.node_id;
     END",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_do_not_carry_trailing_semicolons() {
        // 迁移框架逐条 execute：语句尾部带分号会被当成多条语句
        for (i, s) in STEPS.iter().enumerate() {
            assert!(!s.trim_end().ends_with(';'), "步骤 {i} 尾部不应带分号");
        }
    }

    #[test]
    fn every_node_and_content_write_is_covered() {
        // 三条触发事件 × 两张表，缺一就会出现「改了搜不到」
        let all = STEPS.join("\n");
        for needle in [
            "AFTER INSERT ON nodes",
            "AFTER UPDATE OF title ON nodes",
            "AFTER DELETE ON nodes",
            "AFTER INSERT ON node_contents",
            "AFTER UPDATE OF body ON node_contents",
            "AFTER DELETE ON node_contents",
        ] {
            assert!(all.contains(needle), "缺少同步触发器：{needle}");
        }
    }
}
