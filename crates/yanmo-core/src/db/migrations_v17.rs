//! 迁移 v17：**稿件流转**——`works` 多一列状态，另起一张投稿历史表 `submissions`。
//!
//! ⚠️ 与 v1~v16 同样**一旦发布不可再改**。
//!
//! # 为什么状态挂在 `works` 上
//!
//! 一篇稿子只有**一个**当前状态，与作品同生共死（软删、真删、级联都跟着走），
//! 没有"这篇有几个状态"这种问法。挂在作品上，书架与看板读的是同一行——
//! 这正是"两处同源、别做成两个列表"在数据上的样子。
//!
//! # 为什么投稿历史另起一张表
//!
//! 真正要回答的是"**投过哪几家、还在等谁**"——那是**历史**，会一趟一趟长出来：
//! 同一篇先投甲、被拒、冷冻期过了改投乙。往 `works` 上加 `submitted_to` 这样的列，
//! 第二次投稿就把第一次覆盖掉了，等于把最该记住的东西丢掉。
//!
//! # 为什么不做成 JSON 一列
//!
//! 它要按作品查、要按"还在等回音"筛、要能单独改一条（记错了只改那一条），
//! 还要在未来接多端同步——这三件事都是"表"的形状，不是"一格文本"的形状。
//!
//! # 列与语义
//!
//! - `works.status`：稳定码，取值见 `model::ManuscriptState`（`draft` / `finished` /
//!   `ready` / `submitted` / `accepted` / `rejected` / `published`）。**认不出来就报错**，
//!   不猜着读；老库迁移后一律是 `draft`。
//! - `submissions.result`：稳定码，取值见 `model::SubmissionResult`（`waiting` 等回音 /
//!   `accepted` 过稿 / `rejected` 退稿 / `withdrawn` 自己撤回）。
//! - `submissions.cooldown_until`：**冷冻期截止**（可空＝作者没定期限）。到点仍没回音
//!   视为可改投——但**历史不改写**，那条记录仍是 `waiting`（编辑后来回了话，
//!   历史不能自己先替他说"没回"）。
//! - `submissions.deleted_at`：软删（与作品、章节、碎片同一条纪律：删只是打时间戳）——
//!   记错一条投稿不该连累其余历史。
//!
//! 外键挂在 `works` 上并级联删（书真被清掉时，投稿历史没有留着的理由）；
//! 索引按最常用的两种问法覆盖："这篇投过哪几家"（`work_id, submitted_at DESC`）
//! 与"这篇还在等回音吗"（`work_id, result`）。

/// v17 的静态步骤：建表 + 建索引（都幂等，不需要 `prepare`）。
pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS submissions (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        work_id        INTEGER NOT NULL REFERENCES works(id) ON DELETE CASCADE,
        venue          TEXT    NOT NULL,
        submitted_at   INTEGER NOT NULL,
        result         TEXT    NOT NULL DEFAULT 'waiting',
        cooldown_until INTEGER,
        notes          TEXT    NOT NULL DEFAULT '',
        created_at     INTEGER NOT NULL,
        updated_at     INTEGER NOT NULL,
        deleted_at     INTEGER
    )",
    "CREATE INDEX IF NOT EXISTS idx_submissions_work
        ON submissions(work_id, submitted_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_submissions_open
        ON submissions(work_id, result)",
];

/// 缺 `status` 列就加（默认 `draft`＝老库里的稿子都算草稿）。
///
/// `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能先查 `PRAGMA table_info`
/// 再决定（迁移铁律第 3 条：幂等化）。
pub fn prepare(conn: &rusqlite::Connection) -> crate::error::Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "works", "status")? {
        steps.push("ALTER TABLE works ADD COLUMN status TEXT NOT NULL DEFAULT 'draft'".to_string());
    }
    Ok(steps)
}

/// `table` 上有没有 `column`（`PRAGMA table_info` 的列名在第二列）。
fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> crate::error::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 幂等：连跑两次只该加一次列（第一次加，第二次什么都不做）。
    #[test]
    fn prepare_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE works(id INTEGER PRIMARY KEY)", []).unwrap();
        assert_eq!(prepare(&conn).unwrap().len(), 1, "第一次应当加一列");
        for step in prepare(&conn).unwrap() {
            conn.execute(step.as_str(), []).unwrap();
        }
        assert!(prepare(&conn).unwrap().is_empty(), "加过之后重跑不该再加");
    }
}
