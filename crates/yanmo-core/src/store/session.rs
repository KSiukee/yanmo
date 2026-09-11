//! 编辑器会话：**该编辑哪一章** + **上次是不是正常退出**。
//!
//! 两件事都围绕 `settings` 里的一个键（JSON）展开：
//!
//! - 启动时写「正在运行」，正常退出时改「已正常退出」；
//! - 启动时发现上一个标记还是「正在运行」→ 上次是被杀 / 崩溃。**这就是崩溃检测的全部依据**：
//!   一个字段，不猜、不靠启发式、不依赖文件时间戳；
//! - 标记里同时记着最后编辑的节点与最后落盘的指纹，因此重开能**回到原来那一章**。
//!
//! ⚠️ 诚实的边界：被杀的那一刻**内存里还没落盘的内容无法恢复**（字只在内存里，
//! 没有任何地方写着它）。本任务保证的是两件不同的事：
//! ① 已经落盘的一个字都不少（WAL + 每次写入一个事务）；
//! ② 关窗 / 失焦 / 切后台前**一定先落盘**，存不下去就不放行。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::{Error, Result};
use crate::model::{NodeKind, WorkKind};
use crate::time::now_millis;

/// 会话标记在 `settings` 里的键。
const SESSION_KEY: &str = "session.last";
/// 首次运行时的默认作品名（真正的书架与命名随后接管）。
const DEFAULT_WORK_TITLE: &str = "我的第一篇";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Marker {
    pid: u32,
    started_at: i64,
    heartbeat_at: i64,
    node_id: Option<i64>,
    fingerprint: String,
    /// 上一次是不是正常退出
    clean: bool,
}

/// 上一次会话留下的交代。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    /// 上一次没有正常退出（被杀 / 崩溃）
    pub unclean: bool,
    /// 崩溃前正在编辑的节点
    pub last_node_id: Option<i64>,
    /// 上次活动时间（unix 毫秒）
    pub last_seen_at: Option<i64>,
}

/// 界面当前该挂载的编辑目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTarget {
    pub work_id: i64,
    pub node_id: i64,
    pub title: String,
}

impl Store {
    /// 启动时调用一次：读旧标记 → 写新标记 → 报告上次退得干不干净。
    pub fn begin_session(&mut self) -> Result<SessionReport> {
        let previous = self.read_marker()?;
        let report = SessionReport {
            unclean: previous.as_ref().map(|m| !m.clean).unwrap_or(false),
            last_node_id: previous.as_ref().and_then(|m| m.node_id),
            last_seen_at: previous.as_ref().map(|m| m.heartbeat_at),
        };
        let now = now_millis();
        self.write_marker(&Marker {
            pid: std::process::id(),
            started_at: now,
            heartbeat_at: now,
            node_id: None,
            fingerprint: String::new(),
            clean: false,
        })?;
        Ok(report)
    }

    /// 正常退出：**先留关窗快照，再标记干净退出**。返回是否真的写了新快照。
    pub fn end_session(&mut self, node_id: i64) -> Result<bool> {
        let snapshot_written = self.snapshot_if_changed(node_id, "close")?;
        self.mark_clean()?;
        Ok(snapshot_written)
    }

    /// 放弃这次会话（用户选择"仍然退出"）：只标干净，不写快照。
    pub fn abandon_session(&mut self) -> Result<()> {
        self.mark_clean()
    }

    /// 记下"现在打开的是哪一章"。
    ///
    /// 落盘时会自动记，这里补的是**还没写一个字就被杀**的情况——
    /// 否则重开时只知道上次退得不干净，却不知道人当时坐在哪一章。
    pub fn note_open_node(&self, node_id: i64) -> Result<()> {
        self.note_heartbeat(Some(node_id), None)
    }

    /// 心跳：更新活动时间，顺带记住最后编辑的节点与指纹。
    ///
    /// `None` 表示"这一项保持不变"——它由落盘与读回校验顺带调用，不额外增加界面往返。
    pub(super) fn note_heartbeat(&self, node_id: Option<i64>, fingerprint: Option<&str>) -> Result<()> {
        let Some(mut marker) = self.read_marker()? else {
            return Ok(()); // 没有会话标记（尚未 begin_session）：不凭空造一个
        };
        if let Some(id) = node_id {
            marker.node_id = Some(id);
        }
        if let Some(fp) = fingerprint {
            marker.fingerprint = fp.to_string();
        }
        marker.heartbeat_at = now_millis();
        marker.clean = false;
        self.write_marker(&marker)
    }

    fn mark_clean(&self) -> Result<()> {
        let Some(mut marker) = self.read_marker()? else {
            return Ok(());
        };
        marker.clean = true;
        marker.heartbeat_at = now_millis();
        self.write_marker(&marker)
    }

    /// 读标记。读不出来（没写过 / JSON 坏了）时按**未正常退出**处理——
    /// 宁可多提醒一次，也不要漏报一次崩溃。
    fn read_marker(&self) -> Result<Option<Marker>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SESSION_KEY],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|json| {
            serde_json::from_str::<Marker>(&json).unwrap_or(Marker {
                pid: 0,
                started_at: 0,
                heartbeat_at: 0,
                node_id: None,
                fingerprint: String::new(),
                clean: false,
            })
        }))
    }

    fn write_marker(&self, marker: &Marker) -> Result<()> {
        let json = serde_json::to_string(marker).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![SESSION_KEY, json, now_millis()],
        )?;
        Ok(())
    }

    /// 取（必要时创建）一个可以立刻开始写的编辑目标。
    pub fn ensure_editor_target(&mut self) -> Result<EditorTarget> {
        self.ensure_editor_target_preferring(None)
    }

    /// 同上，但优先回到 `preferred`（崩溃 / 上次退出时正在编辑的那一章）。
    ///
    /// 只有"这个节点还活着、而且承载正文"时才认——不猜、不硬凑。
    pub fn ensure_editor_target_preferring(&mut self, preferred: Option<i64>) -> Result<EditorTarget> {
        if let Some(node_id) = preferred {
            if let Some(target) = self.editor_target_for(node_id)? {
                self.touch_work_opened(target.work_id)?;
                return Ok(target);
            }
        }

        let work_id = match self.list_works()?.first() {
            Some(existing) => existing.id,
            None => self.create_work(WorkKind::Article, DEFAULT_WORK_TITLE)?.id,
        };
        self.ensure_target_in_work(work_id)
    }

    /// 切到某一本书时的落点：**上次在这本书里写的那一章**（记不起来了就给这本书的第一章）。
    ///
    /// "上次在哪一章"与"读到哪了"是同一份记录（光标），所以这里不另存一份——
    /// 一份记录只有一种含义，不会出现两处说法不一致。
    pub fn work_target(&mut self, work_id: i64) -> Result<EditorTarget> {
        if let Some(node_id) = self.cursor_node(work_id)? {
            // 只有"还在、还承载正文"才认——那一章被删了，不能把人送进回收站
            if let Some(target) = self.editor_target_for(node_id)? {
                self.touch_work_opened(target.work_id)?;
                return Ok(target);
            }
        }
        self.ensure_target_in_work(work_id)
    }

    /// 在一本书里找一个能立刻落笔的目标：优先现成的正文节点，没有就补一章。
    fn ensure_target_in_work(&mut self, work_id: i64) -> Result<EditorTarget> {
        super::work::ensure_alive(&self.conn, work_id)?;
        self.touch_work_opened(work_id)?;

        let nodes = self.list_nodes(work_id)?;
        if let Some(node) = nodes.iter().find(|n| n.kind.holds_body()) {
            return Ok(EditorTarget {
                work_id,
                node_id: node.id,
                title: node.title.clone(),
            });
        }

        // 有作品但没有能落正文的节点（例如只有一卷）：补一章，别让作者对着空目录发呆
        let parent = nodes.iter().find(|n| n.parent_id.is_none()).map(|n| n.id);
        let node_id = self.create_node(work_id, parent, NodeKind::Chapter, "第一章")?;
        Ok(EditorTarget {
            work_id,
            node_id,
            title: "第一章".to_string(),
        })
    }

    /// 严格取一个编辑目标：不存在 / 已删除 / 不承载正文 → **明确报错**。
    ///
    /// 与 [`Store::ensure_editor_target_preferring`] 的区别：那个会"退而求其次"给个默认章，
    /// 适合启动引导；而用户明确点"切到这一章"时**绝不能悄悄换地方**。
    pub fn editor_target(&mut self, node_id: i64) -> Result<EditorTarget> {
        let target = self.editor_target_for(node_id)?.ok_or_else(|| {
            Error::Invalid(format!("不能切到该节点（不存在 / 已删除 / 不承载正文）：{node_id}"))
        })?;
        self.touch_work_opened(target.work_id)?;
        Ok(target)
    }

    /// 把某个节点变成编辑目标（节点必须存活、承载正文、且属于活着的作品）。
    fn editor_target_for(&self, node_id: i64) -> Result<Option<EditorTarget>> {
        let row: Option<(i64, String, String)> = self
            .conn
            .query_row(
                "SELECT n.work_id, n.title, n.node_kind
                 FROM nodes n
                 JOIN works w ON w.id = n.work_id AND w.deleted_at IS NULL
                 WHERE n.id = ?1 AND n.deleted_at IS NULL",
                params![node_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match row {
            Some((work_id, title, kind)) if NodeKind::parse(&kind)?.holds_body() => Ok(Some(EditorTarget {
                work_id,
                node_id,
                title,
            })),
            _ => Ok(None),
        }
    }
}

/// 光标与滚动位置：**界面状态，但归数据层存**。
///
/// 理由是它有"资产"属性：崩溃恢复回到原位、以后多作品来回切换秒回原处，都靠它，
/// 存在 WebView 里会随壳的存储一起没（换壳、清缓存就丢）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorCursor {
    /// 选区起点（ProseMirror 文档位置）
    pub anchor: i64,
    /// 选区终点
    pub head: i64,
    /// 编辑区滚动位置
    pub scroll_top: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CursorRecord {
    node_id: i64,
    cursor: EditorCursor,
}

/// 光标记录的 settings 键：**按作品分键**（每本书各自记自己读到哪了）。
///
/// 一本书一个槽：切书回来能秒回原位，而键的数量只随书增加，不会随章节数膨胀。
fn cursor_key(work_id: i64) -> String {
    format!("work.{work_id}.cursor")
}

/// 旧版的单槽键（全库只有一条记录）。**只在还没写过分键记录时兜一次**，
/// 写过一次之后就被清掉——升级上来的老库不该丢掉"上次读到哪了"。
const LEGACY_CURSOR_KEY: &str = "editor.cursor";

impl Store {
    /// 记下"这一章我读到哪了"。失焦 / 切章 / 切书 / 关窗前调用，**不跟着击键走**。
    pub fn save_cursor(&self, node_id: i64, cursor: EditorCursor) -> Result<()> {
        let work_id = self.node_work(node_id)?; // 不往已删除的节点上记位置
        let json = serde_json::to_string(&CursorRecord { node_id, cursor }).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![cursor_key(work_id), json, now_millis()],
        )?;
        // 分键记录已经写上了，旧单槽就没用了：顺手送走，免得下次又兜一次
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", params![LEGACY_CURSOR_KEY])?;
        Ok(())
    }

    /// 取回光标——**只有记的正是这一章、而且这一章还活着时才认**。
    ///
    /// 两道门槛都必要：认错章会把别处的光标套上来；认已删除的章（回收站里）则毫无意义。
    pub fn load_cursor(&self, node_id: i64) -> Result<Option<EditorCursor>> {
        let Ok(work_id) = self.node_work(node_id) else {
            return Ok(None); // 节点没了：没有"读到哪了"这回事，不当错误处理
        };
        let record = match self.read_cursor_record(&cursor_key(work_id))? {
            Some(record) => Some(record),
            // 老库的兜底：单槽记录若是这一章的，照样认（认完下次保存就换成分键）
            None => self.read_cursor_record(LEGACY_CURSOR_KEY)?,
        };
        Ok(record
            .filter(|record| record.node_id == node_id)
            .map(|record| record.cursor))
    }

    /// 这本书上次写的是哪一章（光标记录顺带记着它）——切书时用来秒回原位。
    pub(super) fn cursor_node(&self, work_id: i64) -> Result<Option<i64>> {
        Ok(self.read_cursor_record(&cursor_key(work_id))?.map(|record| record.node_id))
    }

    fn read_cursor_record(&self, key: &str) -> Result<Option<CursorRecord>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?;
        // 记录坏了就当没有，不猜
        Ok(raw.and_then(|json| serde_json::from_str::<CursorRecord>(&json).ok()))
    }
}
