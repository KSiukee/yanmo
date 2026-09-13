//! 作品（`works`）读写。**书架是一等公民**：多作品是默认形态，不是附加功能。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{NodeKind, Work, WorkKind, WorkLanguage};
use crate::time::now_millis;

/// 作品字段列表（顺序与 [`WorkRow`] 对应）。
const COLS: &str =
    "id, kind, title, language, target_words, summary, created_at, updated_at, opened_at";

/// 书架排序口径：**最近打开的在前**；没打开过的按最近编辑。
///
/// 书架与"该编辑哪一本"共用这一份——排序口径只留一处，免得两处慢慢走偏。
const ORDER_BY_OPENED: &str =
    "ORDER BY opened_at IS NULL, opened_at DESC, updated_at DESC, id DESC";

/// 一行的原始取值——先取成朴素类型，再**在 Rust 侧校验**（不在 SQL 里猜着读）。
type WorkRow = (i64, String, String, String, Option<i64>, String, i64, i64, Option<i64>);

/// 书架的一行：作品本身 + 它的规模。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfEntry {
    pub work: Work,
    /// 这本书里章的个数（场景卡这类卡片不算章）
    pub chapters: i64,
    /// 这本书的字数合计（各节点预聚合字数之和，不扫正文）——按词
    pub word_count: i64,
    /// 同上，逐字（含标点）
    pub char_count: i64,
    /// 同上，逐字（不含标点）
    pub chars_no_punct: i64,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn build(row: WorkRow) -> Result<Work> {
    Ok(Work {
        id: row.0,
        kind: WorkKind::parse(&row.1)?,
        title: row.2,
        language: WorkLanguage::parse(&row.3)?,
        target_words: row.4,
        summary: row.5,
        created_at: row.6,
        updated_at: row.7,
        opened_at: row.8,
    })
}

/// 新建作品的根节点模板：**只是给个起点，不是结构约束**（深度不写死）。
///
/// 长篇给一个空卷，方便往里加章；文章与短篇集直接给"单篇"——**根节点即正文，零层级**。
///
/// ⚠️ 长篇的卷名**刻意留空、不写死一个默认名**：名字一旦落库就成了用户数据，
/// 换界面语言后它还是老语言的样子。空名字由界面按当前语言补占位显示
/// （见界面字典里的 `tree.volume_placeholder`），作者一起名就覆盖掉。
fn root_template(kind: WorkKind, work_title: &str) -> (NodeKind, String) {
    match kind {
        WorkKind::Novel => (NodeKind::Volume, String::new()),
        WorkKind::Article | WorkKind::Collection => (NodeKind::Piece, work_title.to_string()),
    }
}

impl Store {
    /// 新建作品：**同一个事务里连根节点一起建**——失败不留半个作品。
    ///
    /// 标题**留空是允许的**（"还没起名"）：首次运行的默认作品就是无名的那一本，
    /// 界面按语言补占位显示。改名则不允许清空（那是作者明确在给这一本起名）。
    pub fn create_work(&mut self, kind: WorkKind, title: &str) -> Result<Work> {
        let title = title.trim();
        let now = now_millis();
        let (root_kind, root_title) = root_template(kind, title);

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO works(kind, title, created_at, updated_at, opened_at)
             VALUES(?1, ?2, ?3, ?3, ?3)",
            params![kind.as_str(), title, now],
        )?;
        let work_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO nodes(work_id, parent_id, node_kind, title, sort_order, created_at, updated_at)
             VALUES(?1, NULL, ?2, ?3, 0, ?4, ?4)",
            params![work_id, root_kind.as_str(), root_title, now],
        )?;
        let root_id = tx.last_insert_rowid();
        tx.commit()?;

        self.record("works", work_id, "create", json!({ "kind": kind.as_str(), "title": title }))?;
        self.record("nodes", root_id, "create", json!({ "work_id": work_id, "root": true }))?;

        Ok(Work {
            id: work_id,
            kind,
            title: title.to_string(),
            // 新书默认中文：研墨的作者以中文写作为主；要写英文/日文，界面上一改就落库
            language: WorkLanguage::Zh,
            target_words: None,
            // 新书还没写简介：空串就是"没写过"（v6 起）
            summary: String::new(),
            created_at: now,
            updated_at: now,
            opened_at: Some(now),
        })
    }

    /// 书架列表：**未删除**的作品，最近打开优先，其次最近编辑。
    pub fn list_works(&self) -> Result<Vec<Work>> {
        let sql = format!("SELECT {COLS} FROM works WHERE deleted_at IS NULL {ORDER_BY_OPENED}");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], read_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(build(row?)?);
        }
        Ok(out)
    }

    /// 书架：一眼看全书——书名之外还带上**章数与字数合计**（一次查完，不为每本书各跑一趟）。
    ///
    /// 规模数字用的是预聚合的 `nodes.word_count`，所以书架不会为了显示去扫正文。
    pub fn shelf(&self) -> Result<Vec<ShelfEntry>> {
        let sql = format!(
            "SELECT {COLS},
                    (SELECT COUNT(*) FROM nodes n
                      WHERE n.work_id = works.id AND n.deleted_at IS NULL AND n.node_kind = 'chapter'),
                    (SELECT COALESCE(SUM(n.word_count), 0) FROM nodes n
                      WHERE n.work_id = works.id AND n.deleted_at IS NULL),
                    (SELECT COALESCE(SUM(n.char_count), 0) FROM nodes n
                      WHERE n.work_id = works.id AND n.deleted_at IS NULL),
                    (SELECT COALESCE(SUM(n.chars_no_punct), 0) FROM nodes n
                      WHERE n.work_id = works.id AND n.deleted_at IS NULL)
               FROM works
              WHERE deleted_at IS NULL
              {ORDER_BY_OPENED}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                (
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ),
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (raw, chapters, word_count, char_count, chars_no_punct) = row?;
            out.push(ShelfEntry { work: build(raw)?, chapters, word_count, char_count, chars_no_punct });
        }
        Ok(out)
    }

    /// 取单个作品（含已删除的也能取到——回收站与恢复要用）。
    pub fn get_work(&self, id: i64) -> Result<Work> {
        let sql = format!("SELECT {COLS} FROM works WHERE id = ?1");
        let raw = self
            .conn
            .query_row(&sql, params![id], read_row)
            .optional()?
            .ok_or_else(|| Error::invalid_with(codes::WORK_NOT_FOUND, [("work_id", id.to_string())]))?;
        build(raw)
    }

    /// 改名。
    pub fn rename_work(&mut self, id: i64, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::invalid(codes::WORK_TITLE_EMPTY));
        }
        let affected = self.conn.execute(
            "UPDATE works SET title = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![title, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        self.record("works", id, "rename", json!({ "title": title }))
    }

    /// 改作品语言——**字数默认口径跟它走**（中文逐字 / 英文按词 / 日文逐字）。
    ///
    /// 只动 `works.language`：不碰任何正文，也不改已存的字数预聚合
    /// （那是"按词"口径算的固定一格，口径切换是显示层的事）。
    pub fn set_work_language(&mut self, id: i64, language: WorkLanguage) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE works SET language = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![language.as_str(), now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        self.record("works", id, "set_language", json!({ "language": language.as_str() }))
    }

    /// 写作品简介（投稿包的大纲要用它）。
    ///
    /// 同"每章一句话"一条规矩：**存作者的原话**，不 trim、不改标点；
    /// 日志只记字数，不把整段话抄进变更留痕。
    pub fn set_work_summary(&mut self, id: i64, summary: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE works SET summary = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![summary, now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        self.record("works", id, "set_summary", json!({ "chars": summary.chars().count() }))
    }

    /// 记一次"打开"——书架排序只看它，不碰编辑时间。
    pub fn touch_work_opened(&mut self, id: i64) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE works SET opened_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        Ok(())
    }

    /// 软删除：只打时间戳，正文与历史都留着（回收站与误删撤销靠它）。
    pub fn soft_delete_work(&mut self, id: i64) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE works SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now_millis(), id],
        )?;
        if affected == 0 {
            return Err(Error::invalid_with(codes::WORK_GONE, [("work_id", id.to_string())]));
        }
        self.record("works", id, "delete", json!({}))
    }

    /// 每卷目标章数——作者自己定的"大概几章一卷"，**按作品分开记**。
    ///
    /// 它只影响目录里"本卷 12/30 章"这行小字，**不改任何结构**；没设过就是 `None`。
    pub fn volume_target(&self, work_id: i64) -> Result<Option<i64>> {
        ensure_alive(&self.conn, work_id)?;
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![volume_target_key(work_id)],
                |r| r.get(0),
            )
            .optional()?;
        // 值坏了就当没设过：这是给人看的进度提示，不值得为它报错挡住界面
        Ok(raw.and_then(|text| text.trim().parse::<i64>().ok()).filter(|n| *n > 0))
    }

    /// 设定 / 清除每卷目标章数：`None`（或 ≤0）= 清掉，回到"没设过"。
    pub fn set_volume_target(&mut self, work_id: i64, chapters: Option<i64>) -> Result<()> {
        ensure_alive(&self.conn, work_id)?;
        let key = volume_target_key(work_id);
        match chapters.filter(|n| *n > 0) {
            Some(count) => {
                self.conn.execute(
                    "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
                    params![key, count.to_string(), now_millis()],
                )?;
            }
            None => {
                self.conn
                    .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            }
        }
        self.record("works", work_id, "set_volume_target", json!({ "chapters": chapters }))
    }
}

/// 每卷目标章数在 `settings` 里的键（按作品分开，互不干扰）。
fn volume_target_key(work_id: i64) -> String {
    format!("work.{work_id}.volume_target")
}

/// 供同层其他模块复用的作品存在性检查（防"往已删除的作品里写东西"）。
pub(super) fn ensure_alive(conn: &Connection, work_id: i64) -> Result<()> {
    let alive: Option<i64> = conn
        .query_row(
            "SELECT id FROM works WHERE id = ?1 AND deleted_at IS NULL",
            params![work_id],
            |r| r.get(0),
        )
        .optional()?;
    match alive {
        Some(_) => Ok(()),
        None => Err(Error::invalid_with(codes::WORK_GONE, [("work_id", work_id.to_string())])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_template_is_zero_level_piece() {
        let (kind, title) = root_template(WorkKind::Article, "我的第一篇");
        assert_eq!(kind, NodeKind::Piece);
        assert_eq!(title, "我的第一篇", "单篇的根节点应当就是这篇文章本身");
    }

    #[test]
    fn novel_template_starts_with_an_unnamed_volume() {
        let (kind, title) = root_template(WorkKind::Novel, "长夜");
        assert_eq!(kind, NodeKind::Volume);
        assert_eq!(title, "", "默认卷名不落库：落了库就固化成某一种语言的名字了");
    }
}
