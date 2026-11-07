//! 编辑落点：**该编辑哪一章** + **读到哪了**（光标与滚动位置）。
//!
//! 从 `session.rs` 拆出来（2026-11-07，那个文件 0.72.2 起就贴着 400 行）：
//! 会话那一半管「上次是不是正常退出」（崩溃检测），这一半管「打开哪一章、光标在哪」——
//! 两者唯一的交集是"标记里记着最后编辑的节点"，读写各归各的。
//!
//! 三条分寸：
//!
//! 1. **落点只认还活着、且承载正文的节点**——不猜、不硬凑；被删的章不能把人送进回收站；
//! 2. **光标按作品分键**（`work.{id}.cursor`）：一本书一个槽，切书回来秒回原位，
//!    键的数量只随书增加、不随章节膨胀；老库的单槽记录只在还没写过新记录时兜一次；
//! 3. **记录坏了就当没记过**（不猜、不修）——界面拿到 `None` 会自己回到第一章。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{NodeKind, WorkKind};
use crate::time::now_millis;

/// 界面当前该挂载的编辑目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTarget {
    pub work_id: i64,
    pub node_id: i64,
    pub title: String,
}

impl Store {
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
            // 首次运行给一本**无名**的书：名字由作者起，界面先显示占位——
            // 不在库里写死一个中文默认名（换语言后它不会跟着变，那就成了脏数据）
            None => self.create_work(WorkKind::Article, "")?.id,
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
        // 先找**最具体的正文节点**：独立长文是「单篇 + 一层节」，落在单篇那层只会看到空壳，
        // 真正的正文在节里；只有没有节时才退回单篇自己（零层级单篇就是这种）。
        if let Some(node) = nodes
            .iter()
            .find(|n| n.kind.holds_body() && n.kind != NodeKind::Piece)
        {
            return Ok(EditorTarget {
                work_id,
                node_id: node.id,
                title: node.title_rendered.clone(),
            });
        }
        if let Some(node) = nodes.iter().find(|n| n.kind.holds_body()) {
            return Ok(EditorTarget {
                work_id,
                node_id: node.id,
                title: node.title_rendered.clone(),
            });
        }

        // 有作品但没有能落正文的节点（例如只有一卷）：补一章，别让作者对着空目录发呆
        let parent = nodes.iter().find(|n| n.parent_id.is_none()).map(|n| n.id);
        // 标题留空 = 交给核心那一份取号实现（别再在这儿写死一个名字，两处迟早不一致）
        let node_id = self.create_node(work_id, parent, NodeKind::Chapter, "")?;
        let title = self.rendered_title(node_id)?;
        Ok(EditorTarget { work_id, node_id, title })
    }

    /// 严格取一个编辑目标：不存在 / 已删除 / 不承载正文 → **明确报错**。
    ///
    /// 与 [`Store::ensure_editor_target_preferring`] 的区别：那个会"退而求其次"给个默认章，
    /// 适合启动引导；而用户明确点"切到这一章"时**绝不能悄悄换地方**。
    pub fn editor_target(&mut self, node_id: i64) -> Result<EditorTarget> {
        let target = self.editor_target_for(node_id)?.ok_or_else(|| {
            Error::invalid_with(codes::NODE_NOT_EDITABLE, [("node_id", node_id.to_string())])
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
            Some((work_id, _, kind)) if NodeKind::parse(&kind)?.holds_body() => {
                Ok(Some(EditorTarget {
                    work_id,
                    node_id,
                    // 章名显示渲染后的那一份（模板留在库里，改名时才编辑它）
                    title: self.rendered_title(node_id)?,
                }))
            }
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
