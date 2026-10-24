//! 迁移 v21：**正文来源账**——`node_source_runs` 一张表（一章一行 JSON）。
//!
//! ⚠️ 与 v1~v20 同样**一旦发布不可再改**。
//!
//! # 这是干什么的
//!
//! 「这一章的字是从哪来的」需要按**字节区间**记（见 `store::source_run`）：
//! 同一段里可能既有键入、又有粘贴、还有模块交付。只有碎片级的一个 `source` 列
//! 回答不了"其中多少是打出来的"，而那正是这一版要如实披露的那一格。
//!
//! # 为什么是独立一张表，不塞进 `node_contents`
//!
//! 与 v16 的 `outline_snapshots` 同一条理由：**变化理由不同**。
//! 正文是稿子本身（读、写、备份、导出都围着它）；来源账是**过程证据**，
//! 它只在落盘时跟着更新，回滚/导入/导出都不该被它拖住。分开之后：
//!
//! - 备份成稿、生成 `.md` 镜像、做导出时**不必**读它（也不该把它带出去）；
//! - 将来的创作留痕包要单独读它、单独校验它；
//! - 老库升上来没有这一行——读不到＝"没记录"，界面如实显示「未记录」，
//!   而不是把老稿子假装成"全是键入"。
//!
//! # 为什么一行 JSON 而不是一张 span 表
//!
//! 一章的来源段通常只有几条到几十条（相邻同源会合并）。一次落盘要整体替换它，
//! 一行 JSON 是一次写入；换成 `(node_id, seq, start, end, source)` 的 span 表，
//! 每次落盘就是几十条 DELETE + INSERT，还得自己维护 seq 的连续性。
//! 这里的读者只有"算一次字数"与将来的留痕包，没有按区间查询的需求——
//! 先按最简单的形状存，等真有查询需求再拆表不迟。
//!
//! 坏 JSON 一律当"没有记录"（`store::source_ledger` 的口径）：不猜、不半信半疑。

/// v21 的静态步骤：一张表 + 一个随节点级联删除的外键。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS node_source_runs (
        node_id    INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
        runs       TEXT    NOT NULL DEFAULT '[]',
        updated_at INTEGER NOT NULL
    )",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// 表能建、能按 node_id 替换；外键指向 nodes 且**级联删除**（节点真删时来源账跟着走）。
    #[test]
    fn table_replaces_by_node_and_cascades_with_the_node() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute("CREATE TABLE nodes(id INTEGER PRIMARY KEY)", []).unwrap();
        conn.execute("INSERT INTO nodes(id) VALUES(1)", []).unwrap();
        for step in STEPS {
            conn.execute(step, []).unwrap();
        }
        conn.execute(
            "INSERT OR REPLACE INTO node_source_runs(node_id, runs, updated_at) VALUES(1, '[]', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO node_source_runs(node_id, runs, updated_at) VALUES(1, '[[0,1,\"typed\"]]', 2)",
            [],
        )
        .unwrap();
        let runs: String = conn
            .query_row("SELECT runs FROM node_source_runs WHERE node_id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(runs.contains("typed"), "同一章应当只有一行（替换而不是追加）");

        // 不真删节点来验级联（稿件表的 DELETE 有守卫盯着）：直接读外键声明。
        // 节点真删走 `store::trash` 的正规出口，那条路径的白名单在 safety_guard 里。
        let mut stmt = conn.prepare("PRAGMA foreign_key_list(node_source_runs)").unwrap();
        let keys: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get::<_, String>(2)?, row.get::<_, String>(6)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(keys, vec![("nodes".to_string(), "CASCADE".to_string())], "来源账要跟节点同生共死");
    }
}
