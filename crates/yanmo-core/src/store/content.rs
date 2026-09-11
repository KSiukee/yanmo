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

/// 一次写入（或核对）得到的字数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentStats {
    /// 字符数（不计空白）
    pub char_count: i64,
    /// 字数（CJK 逐字 + 非 CJK 按串）
    pub word_count: i64,
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

    /// 读正文 + 它的字数（编辑器打开一章用；字数与写入时同一口径）。
    pub fn read_body_with_stats(&self, node_id: i64) -> Result<(String, ContentStats)> {
        let body = self.read_body(node_id)?;
        let stats = ContentStats {
            char_count: text::count_chars(&body),
            word_count: text::count_words(&body),
        };
        Ok((body, stats))
    }

    /// 写正文。
    ///
    /// - 节点不存在或已删除 → 明确报错（不往回收站里写东西）；
    /// - 内容指纹与库里一致 → **原样返回，不做任何写入**；
    /// - 否则一个事务里更新正文、回算字数、更新时间戳。
    pub fn write_body(&mut self, node_id: i64, body: &str) -> Result<ContentStats> {
        self.node_work(node_id)?;

        let hash = text::content_hash(body);
        if let Some((stored_hash, stored_chars)) = self
            .conn
            .query_row(
                "SELECT content_hash, char_count FROM node_contents WHERE node_id = ?1",
                params![node_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
        {
            if stored_hash == hash {
                let words: i64 = self.conn.query_row(
                    "SELECT word_count FROM nodes WHERE id = ?1",
                    params![node_id],
                    |r| r.get(0),
                )?;
                return Ok(ContentStats {
                    char_count: stored_chars,
                    word_count: words,
                });
            }
        }

        let stats = ContentStats {
            char_count: text::count_chars(body),
            word_count: text::count_words(body),
        };
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
            "UPDATE nodes SET word_count = ?1, updated_at = ?2 WHERE id = ?3",
            params![stats.word_count, now, node_id],
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
