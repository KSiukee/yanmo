//! 五线推进度命令域：**这一段在推进哪几条线**的读与写（主线 / 副线 / 感情线 / 悬念线 / 成长线）。
//!
//! 与「大纲」那一个域分开的理由：大纲域管的是**体检与表格整体**（扫一遍、忽略一条、整片粘贴），
//! 这里管的是**一种标注的读写**——与核心那边 `store::lines` 自成一件同一个道理。
//!
//! 三条口径（与核心一致）：
//!
//! - **只做搬运**：参数转换 + 转交核心，**不产句子**（句子在界面字典 `grid.lines.*`）；
//! - **写是整份覆盖**：交上来的就是这一段完整的清单（取消勾＝去掉那一条），
//!   不做"加一条 / 减一条"两套接口（两套接口就有两处幂等语义要对齐）；
//! - **能不能标由核心判**：只有承载正文的章 / 节 / 单篇能标，卷与场景卡当场拒
//!   （`story_line.node_no_body`）——界面不替它猜，也不在这儿另写一套判据。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::StoryLine;

/// 一段的标注：节点 id + 它推进的那几条线（稳定码，按界面上那排的顺序）。
#[derive(Debug, Serialize)]
pub struct NodeLines {
    pub node_id: i64,
    pub lines: Vec<StoryLine>,
}

/// 这本书里**每一段**标了哪些线（没标过的段不出现）——界面把它按节点 id 并到大纲表那一行上。
///
/// 为什么单独一条命令、而不是并进 `outline_rows` 那一屏：标注与"表要摆什么"是两个变化理由，
/// 标注这条链（`store::lines` + 命令行 `lines-set` / `lines-list`）本来就是独立的；
/// 并进去等于让每一张表都背着一份它不一定要用的清单。
#[tauri::command(rename_all = "snake_case")]
pub fn node_lines(data: State<'_, AppData>, work_id: i64) -> Result<Vec<NodeLines>, ApiError> {
    crate::acceptance::note_command("node_lines");
    data.with_store(|store| {
        let mut nodes: Vec<NodeLines> = store
            .node_lines(work_id)?
            .into_iter()
            .map(|(node_id, lines)| NodeLines { node_id, lines })
            .collect();
        // 按节点排：同一个库读两次，界面上拿到的是同一份顺序
        nodes.sort_by_key(|row| row.node_id);
        Ok(nodes)
    })
}

/// 换掉一段的标注（**整份覆盖**），返回库里真有的那一份（稳定序）。
///
/// 空数组＝这一段清空（不是"没动"）：作者把标过的线全取消，是正经的操作。
/// 守规矩的调用点是界面上那一格（勾完一次发一次），脚本来回点也不会留下半吊子状态。
#[tauri::command(rename_all = "snake_case")]
pub fn node_lines_set(
    data: State<'_, AppData>,
    node_id: i64,
    lines: Vec<String>,
) -> Result<Vec<StoryLine>, ApiError> {
    crate::acceptance::note_command("node_lines_set");
    data.with_store(|store| {
        // 认不出的码当场拒（`value.unknown_story_line`）：静默跳过等于把作者标过的那条抹掉
        let wanted: Vec<StoryLine> =
            lines.iter().map(|code| StoryLine::parse(code)).collect::<yanmo_core::Result<_>>()?;
        store.set_node_lines(node_id, &wanted, "author")
    })
}
