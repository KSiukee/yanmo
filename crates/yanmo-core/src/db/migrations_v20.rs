//! 迁移 v20：**分类维度**——`works` 多两列：体裁（`genre`）与标签（`tags`）。
//!
//! ⚠️ 与 v1~v19 同样**一旦发布不可再改**。
//!
//! # 这两列是干什么的
//!
//! 设计里的「分类维度」（体裁、标签、字数档、去向）一直没落地；短篇集做「集内排序」时
//! 那第三档「按主题」因此排不了。这一版先把**人填的那两格**落下来：
//!
//! - `genre`：**体裁**，一行自由文本（`''` = 作者没填）。**核心不设闭集**——
//!   加一个新体裁不该再动一次表结构；界面给常用档 + 允许自填，认不出的照存
//!   （代价是拼错不报错，由下拉收住，不由数据库拦）。
//! - `tags`：**标签**，同样的自由文本，**一行一个**（`''` = 一个都没填）。
//!   为什么不用逗号/空格分隔：标签里带空格、带逗号都是常有的事，一列文本里
//!   再挑一个分隔符，就是给将来的自己埋一颗雷；换行是标签里唯一不该出现的东西
//!   （所以解析时"一行"就是"一个标签"的定义，见 `model::tags`）。
//!
//! # 为什么挂 `works` 上，而不是另起一张 `work_tags` 表
//!
//! 与 v17 的状态同一条理由：**每本书只有一份体裁与一组标签**，与书同生共死
//! （软删、真删、级联都跟着走）。今天要回答的问题只有"这本书的体裁/标签是什么"
//! （集内排序、作品表单回填），一次读一行就够；
//! **将来真要问"哪些书带这个标签"**（书架按标签筛）时，那才是加索引或另起关联表的时机——
//! 那时再加不迟，今天先不预支那份复杂度（v13 的 `node_cast` 是"一问多答"才另起表的）。
//!
//! # 为什么不做成 JSON 一列
//!
//! 一行一个标签的纯文本，用肉眼就能读、用记事本就能改，坏掉的样子也看得懂
//! （多一个空行而已）；JSON 一列则要一个解析器才能看内容，还得处理"解析失败"这一族
//! 与作者无关的错。标签是给人看的一点点元数据，不是结构化数据。
//!
//! # 空格与换行怎么算
//!
//! 存进去的是**归一过的**那一份（去首尾空白、去重、去空行、保留作者写的先后），
//! 归一规则在 `model::tags` 一处——迁移这里只负责把列建出来，老库一律空串
//! （＝"作者还没填过"，不是"这本书没有体裁"）。

/// v20 **没有静态步骤**：两列都得先看库里有没有（`ALTER TABLE ADD COLUMN` 没有
/// `IF NOT EXISTS`），所以全部交给 [`prepare`]。这一条空表是为了让迁移框架那句
/// "既没有静态步骤也没有按需步骤"的断言说得通——不是漏写（v4 / v5 / v6 都是这个形状）。
pub const STEPS: &[&str] = &[];

/// 缺哪一列就补哪一列（默认空串＝没填过）。
///
/// `ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，只能先查 `PRAGMA table_info`
/// 再决定（迁移铁律第 3 条：幂等化）——与 v17 的 `status` 同一手法。
pub fn prepare(conn: &rusqlite::Connection) -> crate::error::Result<Vec<String>> {
    let mut steps = Vec::new();
    for column in ["genre", "tags"] {
        if !column_exists(conn, "works", column)? {
            steps.push(format!("ALTER TABLE works ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"));
        }
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

    /// 幂等：连跑两次只该各加一次列（第二次什么都不做）。
    #[test]
    fn prepare_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE works(id INTEGER PRIMARY KEY)", []).unwrap();
        let first = prepare(&conn).unwrap();
        assert_eq!(first.len(), 2, "第一次应当把两列都加上");
        for step in &first {
            conn.execute(step.as_str(), []).unwrap();
        }
        assert!(prepare(&conn).unwrap().is_empty(), "加过之后重跑不该再加");
    }

    /// 老库升上来是**空串**（＝还没填过），不是 NULL——界面不必再分两种"没有"。
    #[test]
    fn old_works_end_up_with_empty_text() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE works(id INTEGER PRIMARY KEY, title TEXT NOT NULL)", []).unwrap();
        conn.execute("INSERT INTO works(title) VALUES('长夜')", []).unwrap();
        for step in prepare(&conn).unwrap() {
            conn.execute(step.as_str(), []).unwrap();
        }
        let (genre, tags): (String, String) = conn
            .query_row("SELECT genre, tags FROM works WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(genre, "");
        assert_eq!(tags, "");
    }
}
