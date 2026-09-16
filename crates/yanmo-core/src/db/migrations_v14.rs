//! 迁移 v14：**故事总纲**——`works` 上多一列（整本书的立意 / 主线 / 卖点）。
//!
//! ⚠️ 与 v1~v13 同样**一旦发布不可再改**。
//!
//! # 为什么要这一列
//!
//! 大纲表管的是"**这一章**写什么"（一句话 / 四格 / 出场人物 / 伏笔），
//! 而作者在动笔前要想清楚的是"**这本书**讲什么"——橙瓜 / 番茄那一类工具把它做成一段
//! 带模板提示的文字（题材 / 风格 / 主线 / 亮点 / 主角性格），市场调研里也把它判为
//! "整本书层面，不是逐章录入面"（见 `docs/调研-大纲录入形态.md`）。这一列就是那个位置。
//!
//! # 为什么不复用 `summary`（作品简介）
//!
//! 简介是**给别人看的**（投稿 docx 里就摆在书名底下当「简介」），总纲是**给自己看的**
//! （这本书的架子）。两段话的读者不同、写的时间也不同，合成一格之后作者每次都要先想
//! "我这段是写给谁的"——那正是最该避免的含糊。
//!
//! # 为什么挂在 `works` 上，不另起一张表
//!
//! 一本书只有一份、与作品同生共死（软删、真删、级联都跟着走），没有第二行要存，
//! 也没有"这本书的总纲有几条"这种问法。另起一张表只会多一个写入口、多一次 JOIN。
//!
//! # 为什么走 `prepare`
//!
//! `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能先查 `PRAGMA table_info` 再决定
//! （迁移铁律第 3 条：幂等化）——升级到一半断电 / 被杀之后重跑不会撞车。

use rusqlite::Connection;

use crate::error::Result;

/// v14 没有静态步骤：加列必须"先看库才能决定"（见 [`prepare`]）。
pub const STEPS: &[&str] = &[];

/// 缺 `storyline` 列就加（默认空串＝作者还没写过）。
pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "works", "storyline")? {
        steps.push("ALTER TABLE works ADD COLUMN storyline TEXT NOT NULL DEFAULT ''".to_string());
    }
    Ok(steps)
}

/// `table` 上有没有 `column`（`PRAGMA table_info` 的列名在第二列）。
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
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
