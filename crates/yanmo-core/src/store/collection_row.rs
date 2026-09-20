//! 短篇集的**只读那一半**：集里有什么、目标多少、还没归集的散篇有哪些。
//!
//! 与写入口分开的理由同稿件台（`manuscript_row` 与 `manuscript` 分开）：
//! 读的形状与变的理由不一样——这里随时可以多加一列显示，那里每加一条规矩都要过校验。
//!
//! # 一条口径：**软删的集不算集**
//!
//! 集被扔进回收站时，它引用的篇目**不算"已归集"**——那几篇会重新回到"未归集的散篇"里
//! （作者看到的是"这个集没了，篇还散着"）。引用行本身不删：捞回集之后顺序照旧。

use rusqlite::{params, OptionalExtension};

use super::Store;
use crate::error::Result;
use crate::model::{ManuscriptState, Work, WorkKind};

/// 集里的一篇（"还没归集的散篇"用的是同一张形状——界面两处都用它）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleEntry {
    pub id: i64,
    pub title: String,
    /// 这一篇的流转状态（集视图里给它标一下"还没写完"之类）
    pub status: ManuscriptState,
    pub word_count: i64,
    pub char_count: i64,
    pub chars_no_punct: i64,
    pub updated_at: i64,
}

/// 一个集的一屏：篇目（按集内顺序）+ 目标 + 现有规模。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionView {
    pub collection: Work,
    /// 按集内顺序（`sort_order`）
    pub entries: Vec<ArticleEntry>,
    /// 目标篇数（`None` = 没设过）
    pub target_pieces: Option<i64>,
    /// 目标总字数（`None` = 没设过；口径由界面按作品字数口径挑）
    pub target_words: Option<i64>,
}

impl CollectionView {
    /// 集里现有的篇数。
    pub fn pieces(&self) -> i64 {
        self.entries.len() as i64
    }

    /// 集里现有的字数合计（按词口径，与书架同一格）。
    pub fn words(&self) -> i64 {
        self.entries.iter().map(|entry| entry.word_count).sum()
    }

    /// 集里现有的字数合计（逐字含标点）。
    pub fn chars(&self) -> i64 {
        self.entries.iter().map(|entry| entry.char_count).sum()
    }
}

impl Store {
    /// 一个集的一屏：篇目（按集内顺序）+ 目标。
    pub fn collection_view(&self, collection_id: i64) -> Result<CollectionView> {
        let collection = self.get_work(collection_id)?;
        if collection.kind != WorkKind::Collection {
            return Err(crate::error::Error::invalid_with(
                crate::error_codes::codes::COLLECTION_NOT_A_COLLECTION,
                [("work_id", collection_id.to_string())],
            ));
        }
        let sql = format!(
            "SELECT {ARTICLE_COLS} FROM collection_items ci
               JOIN works w ON w.id = ci.article_id AND w.deleted_at IS NULL
               LEFT JOIN nodes n ON n.work_id = w.id AND n.deleted_at IS NULL
              WHERE ci.collection_id = ?1
              GROUP BY w.id
              ORDER BY ci.sort_order, ci.id"
        );
        let entries = self.read_articles(&sql, params![collection_id])?;
        Ok(CollectionView {
            target_pieces: self.collection_target(collection_id, "pieces")?,
            target_words: self.collection_target(collection_id, "words")?,
            collection,
            entries,
        })
    }

    /// 这一篇属于哪几个**活着的**集（升序）。
    pub fn collections_of(&self, article_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT ci.collection_id FROM collection_items ci
               JOIN works c ON c.id = ci.collection_id AND c.deleted_at IS NULL
              WHERE ci.article_id = ?1
              ORDER BY ci.collection_id",
        )?;
        let rows = stmt.query_map(params![article_id], |r| r.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// **还没归集的散篇**（一篇都不在任何活着的集里）：攒集提示与"往集里加一篇"那张清单都用它。
    ///
    /// 最近动过的排前面（刚写完的更容易想收进集子里）。
    pub fn uncollected_articles(&self) -> Result<Vec<ArticleEntry>> {
        let sql = format!(
            "SELECT {ARTICLE_COLS} FROM works w
               LEFT JOIN nodes n ON n.work_id = w.id AND n.deleted_at IS NULL
              WHERE w.kind = 'article' AND w.deleted_at IS NULL
                AND NOT EXISTS (
                      SELECT 1 FROM collection_items ci
                        JOIN works c ON c.id = ci.collection_id AND c.deleted_at IS NULL
                       WHERE ci.article_id = w.id)
              GROUP BY w.id
              ORDER BY w.updated_at DESC, w.id DESC"
        );
        self.read_articles(&sql, [])
    }

    /// 集目标：没设过 / 坏值都是 `None`（那是给人看的参考线，不值得为它报错挡住界面）。
    pub fn collection_target(&self, collection_id: i64, which: &str) -> Result<Option<i64>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![super::collection::target_key(collection_id, which)],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|text| text.trim().parse::<i64>().ok()).filter(|n| *n > 0))
    }

    /// 把一屏篇目读出来（`sql` 只换 WHERE/ORDER，列与汇总口径共用一份）。
    fn read_articles<P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<ArticleEntry>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, title, status, words, chars, chars_no_punct, updated_at) = row?;
            out.push(ArticleEntry {
                id,
                title,
                status: ManuscriptState::parse(&status)?,
                word_count: words,
                char_count: chars,
                chars_no_punct,
                updated_at,
            });
        }
        Ok(out)
    }
}

/// 篇目取哪几列（顺序与 [`Store::read_articles`] 里那一串下标一一对应）。
///
/// 字数走 `nodes` 的预聚合（不扫正文），与书架同一份口径。
const ARTICLE_COLS: &str = "w.id, w.title, w.status,
        COALESCE(SUM(n.word_count), 0),
        COALESCE(SUM(n.char_count), 0),
        COALESCE(SUM(n.chars_no_punct), 0),
        w.updated_at";
