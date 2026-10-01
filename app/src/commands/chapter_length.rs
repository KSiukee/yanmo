//! 章长那一屏：这本书每一章多少字、哪一章算"偏长"（**只读**，不新增任何存储）。
//!
//! 判据在核心一处（`yanmo_core::question::rhythm`，与叩问那条 `rhythm.chapter_very_long`
//! **同源**）——这里只把书里的事实读出来交给界面，**不在这里重算中位数或倍数**：
//! 同一件事两套判据迟早各说各话，而这一屏的字是要念给作者看的。
//!
//! 界面拿它做两件事：目录里给偏长的章一个安静角标；作者真要停下来的那一刻念一句事实。
//! 「就这样写」（这一章以后不再提）**没有第二条命令**：忽略记账本来就是
//! `store::outline_dismiss` 那一族，这一屏把记号原文（`fingerprint`）如实带出来，
//! 界面照抄着交给既有的 `outline_dismiss`——identity 的写法只有核心那一处。

use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::ChapterLengthBoard;

/// 这本书的章长分布 + 逐章判定（**只读**：扫一遍不动一个字节）。
#[tauri::command(rename_all = "snake_case")]
pub fn chapter_lengths(
    data: State<'_, AppData>,
    work_id: i64,
) -> Result<ChapterLengthBoard, ApiError> {
    crate::acceptance::note_command("chapter_lengths");
    data.with_store(|store| store.chapter_lengths(work_id))
}
