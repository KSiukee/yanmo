//! 迁移 v4：作品语言（`works.language`）。
//!
//! ⚠️ 与 v1/v2/v3 同样**一旦发布不可再改**。
//!
//! # 为什么加在 works 上
//!
//! 字数口径本来就与语言相关（中文逐字 / 英文按词 / 日文原稿纸 400 字）：中文界面的作者
//! 写英文小说，字数该按词算。所以语言是**作品的属性**，不是界面设置——随作品一起被
//! 导出、镜像、搬家（见作品镜像那条）。
//!
//! # 存量怎么办
//!
//! 默认 `'zh'`：既有书**一律当中文**（研墨的作者本来就以中文写作为主），
//! 作者在界面上改一次就落库。**不做数据回填**——语言不是从正文猜出来的东西，
//! 猜错比默认中文更糟（一本英文书被猜成中文，字数会一直算错而没人发现）。
//!
//! # 为什么静态步骤是空的
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能**先查 `PRAGMA table_info`** 再决定
//! （迁移铁律第 3 条：幂等化）。照着 v3 的样子放进 [`prepare`]。

use rusqlite::Connection;

use crate::error::Result;

/// v4 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

/// 要加哪些列，**先查再决定**——重跑不会撞 "duplicate column name"。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    if column_exists(conn, "works", "language")? {
        return Ok(Vec::new());
    }
    Ok(vec!["ALTER TABLE works ADD COLUMN language TEXT NOT NULL DEFAULT 'zh'".to_string()])
}

/// `table` 上有没有 `column`（`PRAGMA table_info` 的列名在第二列）。
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // 表名是常量字面量，不是外部输入——PRAGMA 不支持参数占位，只能拼
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
