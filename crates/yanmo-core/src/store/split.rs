//! 拆章：在**作者指定的切点**把一章切成两章——**绝不自动拆**。
//!
//! 它是「章长提醒」（`store::chapter_length`）配的那个动作：提醒只给事实（"这一章 4700 字，
//! 这本书中位 2000 字，是故意的吗？"），要不要拆、从哪儿拆，**始终由作者定**。
//!
//! # 切点为什么是「第几段」而不是「第几个字」
//!
//! 正文在库里存的是**纯文本、空行分段**（口径见 `crate::text::paragraph_spans`）。
//! 所以"在光标处拆"天然就该落在**段落边界**上：把切点定义成「第几段之前切开」，
//! **段落完整性就是结构上保证的**——不存在"按字数硬切把一段拦腰截断"这种事。
//! 光标落在某一段中间时，界面按"那一段的开头"算切点（口径由作者拍板：可预期，
//! 光标丢在目标段落的哪个位置效果都一样）。
//!
//! # 切的时候一个字符都不许动
//!
//! 只允许在切点那一处去掉空行：前半 = `原文[..切点].trim_end()`，后半 = `原文[切点..]`。
//! 段内的硬换行、行尾空白、全角空格**原样留着**——"切点处不丢、不重"是这条任务的验收线，
//! 顺手把正文规范化一遍就不算数了。
//!
//! # 三件事同一个事务
//!
//! 留底（`before_split`）、把前半写回原章、建新章并写后半——**要么全成、要么全不成**。
//! 分开提交的话，中间失败会留下"两章内容加起来少了一截"或"同一段字在两章里都在"，
//! 那是作者最难自查的一种坏法（见 `store::content` 里同一条纪律）。
//!
//! **不记「今日码字」**：拆章没有让作者多写一个字（总数不变，只是重新分了家），
//! 记账那条路（`write_body_counted`）只在编辑器落盘时走。
//!
//! # 只拆正文单位
//!
//! 判据是 [`NodeKind::is_text_spine`]（承载正文、且不是场景卡）——与"数章""能标故事线"
//! **同一个判据**：卷是分组行、场景卡是写这一章时的辅助卡，拆它们没有意义
//! （两半既不会各自成章，也不会进阅读顺序）。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use super::content::{put_body, stats_of};
use super::node::node_kind_in;
use super::node_edit::{create_node_in, move_node_in, sibling_ids, soft_delete_node_in};
use super::snapshot::write_snapshot_in;
use super::Store;
use crate::error::{codes, Error, Result};
use crate::text;

/// 拆章前留底的那个理由码（界面按它翻人话：`snapshots.reason_before_split`）。
pub const SPLIT_SNAPSHOT_REASON: &str = "before_split";

/// 一次拆章的回执——**撤销要的就是它**（界面上那条「已拆成两章 · 撤销」照抄着交回来）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SplitReceipt {
    pub old_node_id: i64,
    pub new_node_id: i64,
    /// 拆之前留的那一份底（撤销认它；作者也能去「版本」里手动回滚）
    pub snapshot_id: i64,
    /// 留在原章的字数
    pub kept_chars: i64,
    /// 分出去的字数
    pub moved_chars: i64,
    /// 新章的标题（作者留空时，这是核心按序号取的那个）
    pub new_title: String,
}

impl Store {
    /// 在**第 `at` 段之前**把这一章切开：前半留在原章，后半进紧跟其后的新章。
    ///
    /// `at` 是段落下标（0 起）：`at == 0` 或 `at >= 段数` 都当场拒——切出一章空的、
    /// 或者"拆了等于没拆"，都不是作者要的（界面会在点之前就把这两种情况拦下）。
    pub fn split_chapter(&mut self, node_id: i64, at: usize, title: &str) -> Result<SplitReceipt> {
        let work_id = self.node_work(node_id)?;
        let kind = node_kind_in(&self.conn, node_id)?;
        if !kind.is_text_spine() {
            return Err(Error::invalid_with(
                codes::CHAPTER_SPLIT_NODE_UNSUPPORTED,
                [("kind", kind.as_str().to_string())],
            ));
        }
        let body = self.read_body(node_id)?;
        let spans = text::paragraph_spans(&body);
        if at == 0 || at >= spans.len() {
            return Err(Error::invalid_with(
                codes::CHAPTER_SPLIT_POINT_INVALID,
                [("at", at.to_string()), ("count", spans.len().to_string())],
            ));
        }
        // **按原文切**：只在切点这一处收掉分隔用的空行（与前半末尾的行尾空白——那本来就不可见，
        // 下次落盘也会被编辑器收掉）。后半从**段头**原样接走：段首空白、段内硬换行一个都不动。
        let cut = spans[at].0;
        let head = body[..cut].trim_end().to_string();
        let tail = body[cut..].to_string();

        let naming = self.naming_style(work_id)?;
        let parent: Option<i64> = self
            .conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", params![node_id], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();
        // 新章落**紧挨着原章之后**（与"在后面加一章"同一条规矩：号是位置的函数）
        let index = sibling_ids(&self.conn, work_id, parent)?
            .iter()
            .position(|id| *id == node_id)
            .map(|position| position + 1)
            .ok_or_else(|| {
                Error::invalid_with(codes::NODE_GONE, [("node_id", node_id.to_string())])
            })?;

        let tx = self.conn.transaction()?;
        let snapshot_id = write_snapshot_in(&tx, node_id, &body, SPLIT_SNAPSHOT_REASON, false)?;
        put_body(&tx, node_id, &head, stats_of(&head))?;
        let new_node_id = create_node_in(&tx, work_id, parent, kind, title, naming)?;
        move_node_in(&tx, new_node_id, parent, index)?;
        put_body(&tx, new_node_id, &tail, stats_of(&tail))?;
        Self::record_in(
            &self.device_id,
            &tx,
            "nodes",
            new_node_id,
            "create",
            json!({ "work_id": work_id, "parent_id": parent, "kind": kind.as_str() }),
        )?;
        // 这一笔落在**原章**上：从哪一章、第几段之前、分出了谁（回溯时一眼看得见）
        Self::record_in(
            &self.device_id,
            &tx,
            "nodes",
            node_id,
            "split",
            json!({ "at": at, "new_node_id": new_node_id, "snapshot_id": snapshot_id }),
        )?;
        tx.commit()?;

        // **渲染后**的章名（作者留空时核心按序号取名，库里存的是模板 `第{$N}章`）——
        // 界面上要念的那一声必须是「第 1 章」这种，绝不能把模板原文漏出去
        //（真机走查逮到的：留空取号时条上念成了「第{$N}章」）。
        let new_title = self.rendered_title(new_node_id)?;
        Ok(SplitReceipt {
            old_node_id: node_id,
            new_node_id,
            snapshot_id,
            kept_chars: text::count_chars(&head),
            moved_chars: text::count_chars(&tail),
            new_title,
        })
    }

    /// 撤销一次拆章：把那一份底写回原章，**新章进回收站**。
    ///
    /// 为什么是"进回收站"而不是"删掉"：拆完以后作者可能在新章里又写了几段，
    /// 撤销只该把两章合回去，**不该顺手把那几段字销毁**——回收站里随时捞得回来。
    ///
    /// 撤销前也留一份底（`before_restore`，与"回滚"同一条纪律）：撤错了还能再换回来。
    pub fn undo_split(&mut self, old_node_id: i64, new_node_id: i64, snapshot_id: i64) -> Result<()> {
        self.node_work(old_node_id)?;
        let saved: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT node_id, body FROM snapshots WHERE id = ?1",
                params![snapshot_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        // 这份底必须是**这一章**的：拿错章的底写回来，等于把两章的字搅在一起
        let Some((_owner, body)) = saved.filter(|(owner, _)| *owner == old_node_id) else {
            return Err(Error::invalid_with(
                codes::SNAPSHOT_NOT_FOUND,
                [("snapshot_id", snapshot_id.to_string())],
            ));
        };
        let current = self.read_body(old_node_id)?;

        let tx = self.conn.transaction()?;
        write_snapshot_in(&tx, old_node_id, &current, "before_restore", false)?;
        put_body(&tx, old_node_id, &body, stats_of(&body))?;
        soft_delete_node_in(&tx, new_node_id)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "nodes",
            old_node_id,
            "split_undo",
            json!({ "new_node_id": new_node_id, "snapshot_id": snapshot_id }),
        )?;
        tx.commit()?;
        Ok(())
    }
}
