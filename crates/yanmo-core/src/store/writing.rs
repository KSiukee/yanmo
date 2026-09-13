//! 每日码字流水（`writing_days`）：**今天写了多少 / 哪一天写了多少**。
//!
//! 三条分寸：
//!
//! 1. **只记"作者敲出来的增减"**：编辑器落盘走 [`Store::write_body_counted`] 才记账；
//!    快照回滚、备份恢复、导入这类"不是今天写的字"走 `write_body`（不记账）——
//!    否则把一章旧稿恢复回来就会在日历上凭空多出一大笔。
//! 2. **净增减**：同一章里删掉的字会扣回来（与作者眼前的字数变化一致），一天下来可能为负。
//! 3. **哪一天由作者时区定**：核心不猜时区，调用方把 `tz_offset_minutes` 递进来
//!    （与 [`crate::time::local_date`] 同一套口径）。
//!
//! 一列一个口径（逐字 / 无标点 / 按词）：口径是显示偏好，作者随时可切；
//! 账本只按一个口径记的话，换了口径历史就跟着变意思了。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::content::ContentStats;
use super::Store;
use crate::error::Result;
use crate::time::{local_date, now_millis, parse_local_date};

/// 某一天的字数（三个口径）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WritingDay {
    /// 本地日期 `2026-09-14`
    pub day: String,
    /// 逐字（含标点）——当天的净增减
    pub chars: i64,
    /// 逐字（不含标点）
    pub chars_no_punct: i64,
    /// 按词
    pub words: i64,
}

impl WritingDay {
    /// 空白的一天（没有记录时用它顶上，界面永远有数可显示）。
    fn empty(day: String) -> Self {
        Self { day, chars: 0, chars_no_punct: 0, words: 0 }
    }
}

/// 读节点上**此刻**的三口径（预聚合值）。写正文前先用它取旧值，好算这一笔增减。
pub(super) fn node_counts(conn: &Connection, node_id: i64) -> Result<ContentStats> {
    let none = ContentStats { char_count: 0, chars_no_punct: 0, word_count: 0 };
    Ok(conn
        .query_row(
            "SELECT char_count, chars_no_punct, word_count FROM nodes WHERE id = ?1",
            params![node_id],
            |r| {
                Ok(ContentStats {
                    char_count: r.get(0)?,
                    chars_no_punct: r.get(1)?,
                    word_count: r.get(2)?,
                })
            },
        )
        .optional()?
        .unwrap_or(none))
}

/// 把一笔增减记进当天（同一天同一本书累加）。
///
/// 三个数全零就不落行——"改了标点但总字数没变"不该在账本上留一条空记录。
pub(super) fn add_delta(
    conn: &Connection,
    work_id: i64,
    day: &str,
    delta: &ContentStats,
) -> Result<()> {
    if delta.char_count == 0 && delta.chars_no_punct == 0 && delta.word_count == 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO writing_days(day, work_id, chars, chars_no_punct, words, updated_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(day, work_id) DO UPDATE SET
             chars          = chars + excluded.chars,
             chars_no_punct = chars_no_punct + excluded.chars_no_punct,
             words          = words + excluded.words,
             updated_at     = excluded.updated_at",
        params![
            day,
            work_id,
            delta.char_count,
            delta.chars_no_punct,
            delta.word_count,
            now_millis()
        ],
    )?;
    Ok(())
}

impl Store {
    /// 今天（按作者时区）的码字——状态栏那行「今日 1,234 / 2,000」用它。
    pub fn writing_today(&self, work_id: Option<i64>, tz_offset_minutes: i32) -> Result<WritingDay> {
        let day = local_date(now_millis(), tz_offset_minutes);
        let mut days = self.writing_between(work_id, &day, &day)?;
        Ok(days.pop().unwrap_or_else(|| WritingDay::empty(day)))
    }

    /// 一个日期区间里**有记录的日子**（没有记录的日子不返回，界面自己补齐空格）。
    ///
    /// `work_id` 给 `None` 就是全部作品合计（"我这阵子一共写了多少"）。
    pub fn writing_between(
        &self,
        work_id: Option<i64>,
        from_day: &str,
        to_day: &str,
    ) -> Result<Vec<WritingDay>> {
        let mut stmt = self.conn.prepare(
            "SELECT day, SUM(chars), SUM(chars_no_punct), SUM(words)
               FROM writing_days
              WHERE day >= ?1 AND day <= ?2 AND (?3 IS NULL OR work_id = ?3)
              GROUP BY day
              ORDER BY day",
        )?;
        let rows = stmt.query_map(params![from_day, to_day, work_id], |r| {
            Ok(WritingDay {
                day: r.get(0)?,
                chars: r.get(1)?,
                chars_no_punct: r.get(2)?,
                words: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 连续码字天数（含今天；今天还没写就从昨天往回数——不然每天早上一睁眼就断签）。
    ///
    /// "写过"的口径是**当天净增长 > 0**：净删的一天不算写。
    pub fn writing_streak(&self, work_id: Option<i64>, tz_offset_minutes: i32) -> Result<i64> {
        let today = local_date(now_millis(), tz_offset_minutes);
        let Some(today_num) = parse_local_date(&today) else {
            return Ok(0);
        };
        // 今天有记录就从今天数，没有就从昨天数
        let today_written: bool = self.conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM writing_days
                  WHERE day = ?1 AND chars > 0 AND (?2 IS NULL OR work_id = ?2))",
            params![today, work_id],
            |r| r.get(0),
        )?;
        let mut expected = if today_written { today_num } else { today_num - 1 };

        let mut stmt = self.conn.prepare(
            "SELECT day FROM writing_days
              WHERE chars > 0 AND (?1 IS NULL OR work_id = ?1)
              GROUP BY day
              ORDER BY day DESC",
        )?;
        let days = stmt.query_map(params![work_id], |r| r.get::<_, String>(0))?;
        let mut streak = 0i64;
        for day in days {
            // 读不动的日期（库被人手改过）当没有这一天——与"坏记录当没设过"同一条规矩
            let Some(num) = parse_local_date(&day?) else { continue };
            if num > expected {
                continue; // 时钟回拨留下的"未来"记录：跳过，不算也断不了签
            }
            if num < expected {
                break; // 断档：从今天往回数到这里为止
            }
            streak += 1;
            expected -= 1;
        }
        Ok(streak)
    }
}
