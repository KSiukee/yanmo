//! 创作流命令域：**你自己记，研墨只替你归位**——碎片池的记 / 看 / 删 / 捞回。
//!
//! 与叩问那条线的分工：问题卡、答案、处置四件套归 [`crate::commands::question`]；
//! 这里只管作者随手记下的那几种（灵感速记 / 事件 / 口述段落），核心会挡着不让
//! 从这里建问题卡或答案（`fragment.kind_not_jotted`）。
//!
//! 面板的数据形状是**一次给全**（列表 + 各筛选项的数字）：它们是同一屏上的同一件事，
//! 分两次问就会让"全部 3 条"和列表里的两条对不上。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::{Fragment, FragmentCount, FragmentKind};
use yanmo_core::store::{NewFragment, Store, FRAGMENTS_PER_BOARD};

/// 创作流面板一次要的全部数据。
#[derive(Debug, Serialize)]
pub struct CreatorBoardDto {
    /// 最近记下的那些（**新的在前**），作者记的那几种
    pub fragments: Vec<Fragment>,
    /// 每一种各有几条（**只数没删的**）——面板上那些筛选项的数字
    pub counts: Vec<FragmentCount>,
}

/// 面板读一次。种类固定是"作者能随手记"那一组：问题与答案有自己的处置面板，
/// 混进来只会让数字和列表各说各话。
fn board(store: &Store, work_id: i64) -> yanmo_core::Result<CreatorBoardDto> {
    let kinds = FragmentKind::JOTTED;
    Ok(CreatorBoardDto {
        fragments: store.fragments(work_id, &kinds, FRAGMENTS_PER_BOARD)?,
        counts: store.fragment_counts(work_id, &kinds)?,
    })
}

/// 记一条碎片。
///
/// `kind` 只认作者自己记的那几种（`idea` / `event` / `dictation`）；`source` 是
/// **怎么记下的**（`typed` / `voice` / `mixed`，闭集，认不出就拒）。`anchors` 是关联锚点
/// （界面把"是在这一章写的"记成 `chapter:<章 id>`）——核心只存不懂它，怎么用它归后面那条线。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_add(
    data: State<'_, AppData>,
    work_id: i64,
    kind: String,
    body: String,
    source: String,
    anchors: Vec<String>,
) -> Result<Fragment, ApiError> {
    crate::acceptance::note_command("fragment_add");
    data.with_store(|store| {
        let kind = FragmentKind::parse(&kind)?;
        let id = store.create_fragment(
            &NewFragment {
                work_id,
                kind,
                body,
                source,
                anchors,
            },
            "author",
        )?;
        // 回执给**库里真有的那一条**（修剪过的原文 + 核心认下的输入方式），不是界面自己回显
        store.fragment(id)
    })
}

/// 看一眼碎片池（不写库）。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_board(data: State<'_, AppData>, work_id: i64) -> Result<CreatorBoardDto, ApiError> {
    crate::acceptance::note_command("fragment_board");
    data.with_store(|store| board(store, work_id))
}

/// 删掉一条（**软删**：库里还留着，`fragment_restore` 能捞回来）。
///
/// 回执是删完之后的最新一屏——界面拿它直接换掉手上那份，不必自己猜少了哪一条。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_delete(data: State<'_, AppData>, id: i64) -> Result<CreatorBoardDto, ApiError> {
    crate::acceptance::note_command("fragment_delete");
    data.with_store(|store| {
        // 回执是它删掉的那一条（软删之后按"取一条"就查不到了），拿它的 work_id 回一屏最新的
        let gone = store.delete_fragment(id, "author")?;
        board(store, gone.work_id)
    })
}

/// 捞回一条删掉的（重复点不算错：已经在的原样返回）。
#[tauri::command(rename_all = "snake_case")]
pub fn fragment_restore(data: State<'_, AppData>, id: i64) -> Result<CreatorBoardDto, ApiError> {
    crate::acceptance::note_command("fragment_restore");
    data.with_store(|store| {
        let back = store.restore_fragment(id, "author")?;
        board(store, back.work_id)
    })
}
