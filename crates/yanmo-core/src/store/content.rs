//! 正文读写（`node_contents`）：与结构分表，**按需拉取**。
//!
//! **「变则写」**：内容指纹没变就什么都不做（不写库、不记日志）。
//! 这是"边写边存"能高频调用的前提——否则每次自动保存都在做无用功。
//!
//! 写入后由触发器同步**检索索引**；三个口径的预聚合字数则是 `put_body` 里
//! **显式 UPDATE 回写** `nodes`（目录树因此不必扫正文就能显示每章字数）。
//! 别去触发器里找字数回写——那里没有（2026-09-15 代码质量评审：轻微 8）。
//!
//! # 来源账与正文同生共死
//!
//! 每一笔真正落库的改动都会顺带更新这一章的**来源账**（`store::source_ledger`）：
//! 旧段怎么搬、新字记给谁，由 `store::source_trace` 按新旧正文比对算出来。
//! 三件事（正文 / 留痕 / 来源账）在**同一个事务**里提交——否则会出现
//! "正文写进去了、来源却还停在上一版"的账，那种账比没有账更坏。
//!
//! 直接改正文、没有观测者的路径（导入 / 采纳灵感 / 镜像同步 / 作答落章）走
//! [`put_body`]，它会**清掉**来源账：读不到＝未记录，绝不拿旧账冒充新正文。

use rusqlite::params;
use rusqlite::OptionalExtension;
use serde_json::json;

use super::source_ledger::{breakdown_of, clear_runs, read_runs, write_runs, SourceBreakdown};
use super::source_trace;
use super::Store;
use crate::error::Result;
use crate::model::{EditOrigin, WriteOrigin};
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

/// 一次落盘的回执：字数 + **这一章现在的来源分布**。
///
/// 来源分布随回执一起给，界面不必再跑一趟"这一章的字从哪来"——
/// 它跟字数本来就是同一笔账的两面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    pub counts: ContentStats,
    pub sources: SourceBreakdown,
}

/// 从正文算三个口径——**唯一一处**（规则本体在 `text::WordCaliber` / `text::tally`，这里只调它）。
pub(super) fn stats_of(body: &str) -> ContentStats {
    let tally = text::tally(body);
    ContentStats {
        char_count: tally.chars,
        chars_no_punct: tally.chars_no_punct,
        word_count: tally.words,
    }
}

/// 把一版正文落进库：正文行 + 三个口径回写到 `nodes`，**并清掉来源账**。
///
/// **只管这几条语句**——指纹比对、记账、留痕都在调用方。拆出这么一小块是因为
/// "从成稿导入"要在**同一个事务**里连写几百章（每章各起一个事务就是几百次提交），
/// 而写入的口径必须与编辑器落盘那条路**逐字一致**，所以只留这一份实现。
///
/// 清来源账是刻意的：走这条路的要么是**新建**正文（从成稿导入，没有旧账可保），
/// 要么是**来源已经接不回去**的整段还原（撤销拆章——快照里只存正文、不存来源段）。
/// 清掉之后界面如实显示「未记录」，比拿旧账冒充新正文好。要保留账的路径走
/// [`put_body_with_runs`]（编辑器 / 拆章）或 [`trace_body_change`]（镜像采纳这类外部改动）。
pub(super) fn put_body(
    tx: &rusqlite::Connection,
    node_id: i64,
    body: &str,
    stats: ContentStats,
) -> Result<()> {
    put_body_raw(tx, node_id, body, stats)?;
    clear_runs(tx, node_id)?;
    Ok(())
}

/// 同上，但**把这一版正文的来源账一起写进去**（编辑器落盘 / 拆章用）。
pub(super) fn put_body_with_runs(
    tx: &rusqlite::Connection,
    node_id: i64,
    body: &str,
    stats: ContentStats,
    runs: &[super::source_run::Run],
) -> Result<()> {
    put_body_raw(tx, node_id, body, stats)?;
    write_runs(tx, node_id, runs)?;
    Ok(())
}

/// 直接改正文、但**想保住没动到的字的来源**时用：按新旧比对算一版新段。
///
/// 与 [`put_body`]（清账）的区别：镜像是"外部把整章改了一遍"，中间没动的段落
/// 不该因为这次采纳就全变成「未记录」。`origin` 说这一笔是什么——外部改动没有
/// 观测者，传 `Unknown`。
pub(super) fn trace_body_change(
    conn: &rusqlite::Connection,
    node_id: i64,
    old: &str,
    new: &str,
    origin: EditOrigin,
) -> Result<Vec<super::source_run::Run>> {
    let runs = read_runs(conn, node_id)?.unwrap_or_default();
    Ok(source_trace::apply(&runs, old, new, origin))
}

fn put_body_raw(
    tx: &rusqlite::Connection,
    node_id: i64,
    body: &str,
    stats: ContentStats,
) -> Result<()> {
    let now = now_millis();
    tx.execute(
        "INSERT INTO node_contents(node_id, body, content_hash, char_count, updated_at)
         VALUES(?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(node_id) DO UPDATE SET
             body = excluded.body,
             content_hash = excluded.content_hash,
             char_count = excluded.char_count,
             updated_at = excluded.updated_at",
        params![node_id, body, text::content_hash(body), stats.char_count, now],
    )?;
    tx.execute(
        // 三个口径**各存一列**：目录树 / 卷合计 / 书架要"一眼看字数"，不能每次去扫正文
        "UPDATE nodes SET word_count = ?1, char_count = ?2, chars_no_punct = ?3, updated_at = ?4
         WHERE id = ?5",
        params![stats.word_count, stats.char_count, stats.chars_no_punct, now, node_id],
    )?;
    Ok(())
}

/// 落盘那一笔的留痕（`op_log.payload`）：字数 + **来源**（模块交付还带署名）。
///
/// 刻意不把正文或段写进来：op-log 是**事件**账，不是第二份正文；
/// 来源的区间细节在 `node_source_runs`，这里只留"这一笔是什么来源"这句话。
fn write_payload(stats: ContentStats, origin: &WriteOrigin) -> serde_json::Value {
    let mut payload = json!({
        "char_count": stats.char_count,
        "chars_no_punct": stats.chars_no_punct,
        "word_count": stats.word_count,
        "origin": origin.kind.as_str(),
    });
    if origin.clipboard_chars > 0 {
        payload["clipboard_chars"] = json!(origin.clipboard_chars);
    }
    if let Some(stamp) = &origin.stamp {
        payload["module_id"] = json!(stamp.module_id);
        payload["model"] = json!(stamp.model);
    }
    payload
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
    ///
    /// **不记账**：快照回滚、备份恢复、导入这类"不是作者今天敲出来的字"走这条。
    /// 来源账按 `unknown` 记（**没有观测者**就不假装知道）；编辑器落盘走
    /// [`Store::write_body_counted`]。
    pub fn write_body(&mut self, node_id: i64, body: &str) -> Result<ContentStats> {
        Ok(self.write_body_inner(node_id, body, None, None, &WriteOrigin::unknown())?.counts)
    }

    /// 编辑器落盘：写正文，**并把这笔增减记进「每日码字」**。
    ///
    /// `tz_offset_minutes` 决定算哪一天（作者本地时区相对 UTC 的偏移，东八区 = 480）。
    /// `origin` 是壳观测到的**这一笔的来源**（键入 / 粘贴 / 模块 / …）；壳没看见就是
    /// `unknown`。模块交付的署名在这里校验：没有模块 id 与模型名的交付**直接拒收**。
    /// 记账、正文、留痕、来源账**同一个事务**：要么全成，要么全不成。
    pub fn write_body_counted(
        &mut self,
        node_id: i64,
        body: &str,
        tz_offset_minutes: i32,
        origin: WriteOrigin,
    ) -> Result<WriteOutcome> {
        self.write_body_inner(node_id, body, Some(tz_offset_minutes), None, &origin)
    }

    /// `extra_log`：要与这次写正文**同一个事务**提交的额外留痕（快照回滚用）——
    /// 回滚报失败必须意味着真的没回滚，否则界面不会把编辑器换成旧正文（评审：中等 6）。
    /// 内容没变（指纹相同）时整笔跳过，额外留痕也一样不写：那本来就没发生改动。
    ///
    /// `origin` 同时决定**来源账**里这一笔新字记给谁；它先过一遍
    /// [`WriteOrigin::validate`]——拒收发生在写库之前，不会留下"写了一半"的正文。
    pub(super) fn write_body_inner(
        &mut self,
        node_id: i64,
        body: &str,
        count_tz: Option<i32>,
        extra_log: Option<(&str, &str, serde_json::Value)>,
        origin: &WriteOrigin,
    ) -> Result<WriteOutcome> {
        origin.validate()?;
        // 这一章属于哪本书——记账要按书记（日历里可以只看一本书）
        let work_id = self.node_work(node_id)?;

        let hash = text::content_hash(body);
        let stats = stats_of(body);
        let old_body = match self
            .conn
            .query_row(
                "SELECT content_hash, body FROM node_contents WHERE node_id = ?1",
                params![node_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            Some((stored_hash, stored_body)) => {
                if stored_hash == hash {
                    // 正文一个字没变：**不写库**（这是"边写边存"能高频调用的前提）。
                    // 但字数照样**现算**返回——口径规则修过之后（如假名按逐字），
                    // 库里那份预聚合值可能是旧规则的，不该拿它糊弄界面。
                    let runs = read_runs(&self.conn, node_id)?.unwrap_or_default();
                    return Ok(WriteOutcome { counts: stats, sources: breakdown_of(body, &runs) });
                }
                Some(stored_body)
            }
            None => None,
        };

        // 来源账在**写正文之前**算：这时库里还是旧正文。
        // 事务只包"写"，不包这一步读——单实例锁保证没有第二个进程在改同一章。
        let old_text = old_body.as_deref().unwrap_or("");
        let old_runs = read_runs(&self.conn, node_id)?.unwrap_or_default();
        let new_runs = source_trace::apply(&old_runs, old_text, body, origin.kind);

        let now = now_millis();
        let tx = self.conn.transaction()?;
        // 记账前先拿旧值：这一笔增减 = 新字数 − 库里旧字数（在同一个事务里读，不会被别人插队）
        let previous = match count_tz {
            Some(_) => Some(super::writing::node_counts(&tx, node_id)?),
            None => None,
        };
        put_body_with_runs(&tx, node_id, body, stats, &new_runs)?;
        if let (Some(tz), Some(previous)) = (count_tz, previous) {
            let delta = ContentStats {
                char_count: stats.char_count - previous.char_count,
                chars_no_punct: stats.chars_no_punct - previous.chars_no_punct,
                word_count: stats.word_count - previous.word_count,
            };
            super::writing::add_delta(&tx, work_id, &crate::time::local_date(now, tz), &delta)?;
        }
        // 留痕与心跳都收进**同一个事务**：以前是提交之后再写，op-log 或心跳写不进去
        // （磁盘满最典型）就把"正文其实已经存好了"报成保存失败，作者据此重试
        // （评审：中等 6）。日志与数据同生共死，也不再有这种中间态。
        Self::record_in(
            &self.device_id,
            &tx,
            "node_contents",
            node_id,
            "write",
            write_payload(stats, origin),
        )?;
        if let Some((entity, op, payload)) = extra_log {
            Self::record_in(&self.device_id, &tx, entity, node_id, op, payload)?;
        }
        // 顺带记心跳与"最后落盘的是哪一章"，**不额外增加界面往返**（崩溃检测靠它）
        Self::note_heartbeat_in(&tx, Some(node_id), Some(&hash))?;
        tx.commit()?;
        Ok(WriteOutcome { counts: stats, sources: breakdown_of(body, &new_runs) })
    }

    /// 把三个字数口径**回填**到节点上——老库只有「按词」那一列。
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
        // 界面每几秒就会调它一次，正好当作"我还活着"的心跳，不必再加一个新命令。
        // 但只能推进时间戳：**这是一次读**，不能把"干净退出"标记撤掉
        //（评审：中等 11——关窗后的一次校验轮询会让下次启动误报崩溃）。
        self.touch()?;
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
