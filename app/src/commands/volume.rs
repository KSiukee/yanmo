//! 分卷命令域：**提议 / 在这里收卷 / 撤卷**。
//!
//! 界面只问两件事："这一章的卷长口径是什么""这一章后面要不要提一句收卷"；
//! 阈值怎么算、卷边界怎么定、结构怎么动，全在核心（`yanmo_core::volume` 与
//! `Store::close_volume` / `Store::dissolve_volume`）——壳这一层只做取值转换。
//!
//! 与 [`crate::commands::tree`] 的分工：目录树的命令管"看得见的结构"（建 / 改名 / 拖 / 删），
//! 这里管"成卷这一件事"的编排入口。它最终仍走核心那两条既有的结构操作。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::Store;
use yanmo_core::volume::{VolumePlan, VolumeSpot};

/// 一本书的分卷口径（界面显示「本卷 12/30 章」的分母就是 `effective`）。
#[derive(Debug, Serialize)]
pub struct VolumePlanDto {
    /// 作者设的「大概几章一卷」（没设过是 null）
    pub target: Option<i64>,
    /// 从他收好的卷学到的中位数（样本 ≥2 才有）
    pub learned: Option<i64>,
    /// 真正在用的阈值（学到的优先，其次作者设的）
    pub effective: Option<i64>,
}

impl From<VolumePlan> for VolumePlanDto {
    fn from(plan: VolumePlan) -> Self {
        Self { target: plan.target, learned: plan.learned, effective: plan.effective }
    }
}

/// 收卷提议：**在这一章之后**可以收卷了。
#[derive(Debug, Serialize)]
pub struct VolumeOfferDto {
    pub plan: VolumePlanDto,
    /// 到这一章为止这一卷已有几章
    pub count: i64,
    /// 提前（early）/ 正好（on_target）/ 延后（late）——界面据它换话术
    pub phase: String,
    /// 收卷点落在哪一卷上（null = 章还散在根上）——界面按它记住"这一卷问过了"
    pub container: Option<i64>,
}

impl From<VolumeSpot> for VolumeOfferDto {
    fn from(spot: VolumeSpot) -> Self {
        Self {
            plan: spot.offer.plan.into(),
            count: spot.offer.count,
            phase: spot.offer.phase.as_str().to_string(),
            container: spot.container,
        }
    }
}

/// 收卷的结果：新卷是谁、有没有顺手起第一章（界面把光标落过去）。
#[derive(Debug, Serialize)]
pub struct CloseVolumeDto {
    pub volume_id: i64,
    pub opened_chapter: Option<i64>,
    pub moved: i64,
}

/// 撤卷的结果：抬回去几项、并进了哪一卷（null = 抬到父层原位）。
#[derive(Debug, Serialize)]
pub struct DissolveVolumeDto {
    pub moved: i64,
    pub merged_into: Option<i64>,
}

/// 这本书的分卷口径（作者没设过、也还没历史时，`effective` 是 null）。
#[tauri::command(rename_all = "snake_case")]
pub fn volume_plan(data: State<'_, AppData>, work_id: i64) -> Result<VolumePlanDto, ApiError> {
    data.with_store(|store: &mut Store| store.volume_plan(work_id).map(VolumePlanDto::from))
}

/// 该不该在**这一章之后**提一句收卷（null = 不用提）。
#[tauri::command(rename_all = "snake_case")]
pub fn volume_offer(data: State<'_, AppData>, node_id: i64) -> Result<Option<VolumeOfferDto>, ApiError> {
    data.with_store(|store: &mut Store| {
        Ok(store.volume_offer(node_id)?.map(VolumeOfferDto::from))
    })
}

/// **在这里收卷**（卷名留空 = 按位置渲染成「第 N 卷」，之后能在树上就地改名）。
#[tauri::command(rename_all = "snake_case")]
pub fn volume_close(
    data: State<'_, AppData>,
    node_id: i64,
    title: String,
) -> Result<CloseVolumeDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let receipt = store.close_volume(node_id, &title)?;
        Ok(CloseVolumeDto {
            volume_id: receipt.volume_id,
            opened_chapter: receipt.opened_chapter,
            moved: receipt.moved as i64,
        })
    })
}

/// **撤卷**：取消这一卷的分卷，里面的东西按原顺序还回去（正文一个字不动）。
#[tauri::command(rename_all = "snake_case")]
pub fn volume_dissolve(data: State<'_, AppData>, volume_id: i64) -> Result<DissolveVolumeDto, ApiError> {
    data.with_store(|store: &mut Store| {
        let receipt = store.dissolve_volume(volume_id)?;
        Ok(DissolveVolumeDto { moved: receipt.moved as i64, merged_into: receipt.merged_into })
    })
}
