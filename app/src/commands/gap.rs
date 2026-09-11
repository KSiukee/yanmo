//! 删章路标命令：点「+」时先问一嘴"这一层是不是少了一章"。
//!
//! 规则全在核心的 `store::gap`：这里只做**参数转换**，不掺第二套判断。
//! 三个动作对应作者的三条路：`check`（摆出空缺）→ `answer`（稍后 / 不用了）
//! 或 `fill`（在原位补写一个空章）。

use serde::Serialize;
use tauri::State;

use crate::storage::AppData;
use yanmo_core::store::{ChapterGap, GapAnswer, Store};

/// 摆给作者的一处空缺（弹窗上那几行字就照它写）。
#[derive(Debug, Serialize)]
pub struct GapDto {
    /// 回收站里那一条（「去回收站看看」用它）
    pub node_id: i64,
    /// 缺在哪一层；`None` = 根级
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    /// 第几号
    pub serial: i64,
    /// 原来叫什么
    pub title: String,
    /// 什么时候删的（unix 毫秒）
    pub deleted_at: i64,
    /// 旧稿多少字（让作者知道"字还在"）
    pub word_count: i64,
}

impl From<ChapterGap> for GapDto {
    fn from(gap: ChapterGap) -> Self {
        Self {
            node_id: gap.node_id,
            parent_id: gap.parent_id,
            parent_title: gap.parent_title,
            serial: gap.serial,
            title: gap.title,
            deleted_at: gap.deleted_at,
            word_count: gap.word_count,
        }
    }
}

/// 这一层现在该不该问一句；不该问就是 `null`（界面据此决定要不要弹窗）。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_gap_check(
    data: State<'_, AppData>,
    work_id: i64,
    parent_id: Option<i64>,
) -> Result<Option<GapDto>, String> {
    data.with_store(|store: &mut Store| {
        Ok(store.gap_in_layer(work_id, parent_id)?.map(GapDto::from))
    })
}

/// 记下作者的主意：`deferred`（稍后再说）/ `ignored`（不用了，别再问）。
///
/// 三态机在核心那一处：第二次「稍后」会自动降为「不用了」。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_gap_answer(
    data: State<'_, AppData>,
    node_id: i64,
    answer: String,
) -> Result<(), String> {
    data.with_store(|store: &mut Store| store.answer_gap(node_id, GapAnswer::parse(&answer)?))
}

/// 补写：在原来的层、用原来的名字与位置新建一个**空章**，返回新章 id。
#[tauri::command(rename_all = "snake_case")]
pub fn tree_fill_gap(data: State<'_, AppData>, node_id: i64) -> Result<i64, String> {
    data.with_store(|store: &mut Store| store.fill_gap_chapter(node_id))
}
