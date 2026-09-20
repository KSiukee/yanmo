//! 流程（SOP）命令域：作者**自己的写作流程**那套房子的读写。
//!
//! 规则全在核心的 `store::sop`（两层继承「书的覆盖 → 全局 → 内置那套起手流程」、
//! 层内只存改过的项、坏记录当没设过、每次改动同事务留痕）：这里只做参数转换 +
//! 把"读出来给人用的那份"递回去。
//!
//! 三条口径与核心一致（也是这个功能的铁律）：
//!
//! 1. **绝不强制顺序**：这里没有任何"下一步"闸门——上移 / 下移 / 跳过 / 删掉 / 加一步
//!    都是平的，作者想怎么走怎么走；
//! 2. **不做打卡**：勾与不勾只是作者自己的记号，核心与壳**都不统计、不提醒、不惩罚**
//!    （库里也不存连续天数）；
//! 3. **核心零文案**：`name` / `note` / 检查项 `text` 没改过就是 `None`，
//!    界面按 `sop.step.<id>.*` 去字典取默认说法。

use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{ResolvedSop, Sop, Store};

/// 读流程：`work_id` 给 null 就是"我平时的写法"那份；给某本书先看它有没有单独设过。
#[tauri::command(rename_all = "snake_case")]
pub fn sop_read(data: State<'_, AppData>, work_id: Option<i64>) -> Result<ResolvedSop, ApiError> {
    crate::acceptance::note_command("sop_read");
    data.with_store(|store: &mut Store| store.sop(work_id))
}

/// 写这一层的流程：**整份替换**（这条流程就是它给你的样子），写完**回读一次**
/// ——界面显示的永远是库里那份（校验夹过、空清单当删掉都在核心做了）。
#[tauri::command(rename_all = "snake_case")]
pub fn sop_write(
    data: State<'_, AppData>,
    work_id: Option<i64>,
    sop: Sop,
) -> Result<ResolvedSop, ApiError> {
    crate::acceptance::note_command("sop_write");
    data.with_store(|store: &mut Store| {
        store.set_sop(work_id, sop)?;
        store.sop(work_id)
    })
}

/// 清掉这一层的流程（书的覆盖则是"回到继承全局"）；历史不涂改，回滚是再写一次。
#[tauri::command(rename_all = "snake_case")]
pub fn sop_reset(data: State<'_, AppData>, work_id: Option<i64>) -> Result<ResolvedSop, ApiError> {
    crate::acceptance::note_command("sop_reset");
    data.with_store(|store: &mut Store| {
        store.reset_sop(work_id)?;
        store.sop(work_id)
    })
}
