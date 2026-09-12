//! 正文读写（`node_contents`）：与结构分表，**按需拉取**。
//!
//! **「变则写」**：内容指纹没变就什么都不做（不写库、不记日志）。
//! 这是"边写边存"能高频调用的前提——否则每次自动保存都在做无用功。
//!
//! 写入后由触发器同步检索索引，并把预聚合字数回写到 `nodes.word_count`
//! （目录树因此不必扫正文就能显示每章字数）。

use rusqlite::params;
use rusqlite::OptionalExtension;
use serde_json::json;

use super::Store;
use crate::error::Result;
use crate::text;
use crate::time::now_millis;

/// 一次写入（或核对）得到的字数——**三个口径都给**，界面按作者选的那个显示。
///
/// 三个数一起回，是因为口径是可切换的显示偏好：界面切换时不该再跑一趟核心。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentStats {
    /// 逐字（含标点）：非空白、非零宽的字符
    pub char_count: i64,
    /// 逐字（不含标点）：汉字 / 假名 / 谚文 / 字母 / 数字
    pub chars_no_punct: i64,
    /// 按词：CJK（表意·假名·谚文）逐字 + 其它连续字母数字串计 1
    pub word_count: i64,
}

/// 从正文算三个口径——**唯一一处**（规则本体在 `text::WordCaliber`，这里只调它）。
pub(super) fn stats_of(body: &str) -> ContentStats {
    ContentStats {
        char_count: text::count_chars(body),
        chars_no_punct: text::count_chars_no_punct(body),
        word_count: text::count_words(body),
    }
}

impl Store {
    /// 读正文——**懒加载入口**。没有正文的节点返回空串。
    pub fn read_body(&self, node_id: i64) -> Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT body FROM node_contents WHERE node_id = ?1",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    /// 读正文 + 它的字数（编辑器打开一章用）。
    ///
    /// **现算**，不读库里的预聚合值：口径规则本身会随版本修正（比如假名/谚文从
    /// "一个词"改成"逐字"），现算的才对得上作者眼前这一版。库里的预聚合值见
    /// `nodes.word_count`，它是目录树用的，靠写入时刷新。
    pub fn read_body_with_stats(&self, node_id: i64) -> Result<(String, ContentStats)> {
        let body = self.read_body(node_id)?;
        let stats = stats_of(&body);
        Ok((body, stats))
    }

    /// 写正文。
    ///
    /// - 节点不存在或已删除 → 明确报错（不往回收站里写东西）；
    /// - 内容指纹与库里一致 → **不写库、不记日志**，但仍返回**现算**的三个字数；
    /// - 否则一个事务里更新正文、回算字数、更新时间戳。
    pub fn write_body(&mut self, node_id: i64, body: &str) -> Result<ContentStats> {
        self.node_work(node_id)?;

        let hash = text::content_hash(body);
        let stats = stats_of(body);
        if let Some(stored_hash) = self
            .conn
            .query_row(
                "SELECT content_hash FROM node_contents WHERE node_id = ?1",
                params![node_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            if stored_hash == hash {
                // 正文一个字没变：**不写库**（这是"边写边存"能高频调用的前提）。
                // 但字数照样**现算**返回——口径规则修过之后（如假名按逐字），
                // 库里那份预聚合值可能是旧规则的，不该拿它糊弄界面。
                return Ok(stats);
            }
        }

        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO node_contents(node_id, body, content_hash, char_count, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(node_id) DO UPDATE SET
                 body = excluded.body,
                 content_hash = excluded.content_hash,
                 char_count = excluded.char_count,
                 updated_at = excluded.updated_at",
            params![node_id, body, hash, stats.char_count, now],
        )?;
        tx.execute(
            // 三个口径**各存一列**：目录树 / 卷合计 / 书架要"一眼看字数"，不能每次去扫正文
            "UPDATE nodes SET word_count = ?1, char_count = ?2, chars_no_punct = ?3, updated_at = ?4
             WHERE id = ?5",
            params![stats.word_count, stats.char_count, stats.chars_no_punct, now, node_id],
        )?;
        tx.commit()?;

        self.record(
            "node_contents",
            node_id,
            "write",
            json!({ "char_count": stats.char_count, "word_count": stats.word_count }),
        )?;
        // 顺带记心跳与"最后落盘的是哪一章"，**不额外增加界面往返**（崩溃检测靠它）
        self.note_heartbeat(Some(node_id), Some(&hash))?;
        Ok(stats)
    }

    /// 把三个字数口径**回填**到节点上（#128）——老库只有「按词」那一列。
    ///
    /// 只在没做过时跑一次：做完在 `settings` 里留标记；中途被杀就下次重跑（重算幂等，
    /// 因为它是从正文原样算出来的，不是累加）。**必须过 Rust**——CJK 口径 SQL 算不了。
    ///
    /// 放在这里而不是迁移里，是因为它要读正文、要用 `text` 的规则；迁移只负责加列。
    pub(super) fn backfill_node_counts(&mut self) -> Result<()> {
        const MARKER: &str = "nodes.counts_backfilled";
        let done: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![MARKER],
                |r| r.get(0),
            )
            .optional()?;
        if done.is_some() {
            return Ok(());
        }

        // 先全读出来再写：同一条连接上不能边遍历结果集边更新
        let rows: Vec<(i64, String)> = {
            let mut stmt = self.conn.prepare("SELECT node_id, body FROM node_contents")?;
            let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            mapped.collect::<rusqlite::Result<Vec<_>>>()?
        };

        let now = now_millis();
        let tx = self.conn.transaction()?;
        for (node_id, body) in &rows {
            let stats = stats_of(body);
            tx.execute(
                "UPDATE nodes SET word_count = ?1, char_count = ?2, chars_no_punct = ?3
                 WHERE id = ?4",
                params![stats.word_count, stats.char_count, stats.chars_no_punct, node_id],
            )?;
            // 顺手把正文表里那一列也刷新：口径规则修过（零宽字符那次），老值可能偏大
            tx.execute(
                "UPDATE node_contents SET char_count = ?1 WHERE node_id = ?2",
                params![stats.char_count, node_id],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![MARKER, "1", now],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 库里正文的**内容指纹**——「写后读回校验」用它比对，**不必把整章文本再传一遍**。
    ///
    /// 没有正文行时返回空串（调用方据此区分"从未写过"与"写过但内容不同"）。
    pub fn body_fingerprint(&self, node_id: i64) -> Result<String> {
        // 界面每几秒就会调它一次，正好当作"我还活着"的心跳，不必再加一个新命令
        self.note_heartbeat(None, None)?;
        Ok(self
            .conn
            .query_row(
                "SELECT content_hash FROM node_contents WHERE node_id = ?1",
                params![node_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }
}
