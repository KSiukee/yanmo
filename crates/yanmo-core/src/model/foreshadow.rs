//! 伏笔：**埋下的一条线头**——一句话、埋在哪一章、收了没有。
//!
//! 它是大纲冲突检测的数据源之一：埋了太久没收的线头，只有**记成结构化的一条**才判得动。
//!
//! # 为什么另起一张表
//!
//! 与设定卡同一条理由、但更硬：伏笔有**两个章节锚点**（埋在哪、收在哪）与一条
//! **小生命周期**（埋着 / 收了 / 不写了）。碎片统一表（`fragments`）装得下"一句话 + 元数据"，
//! 装不下"两个锚点 + 状态"——硬塞就得再补两列，那已经不是碎片了。
//!
//! # 生命周期只有三个态，且**不写了也是正经结局**
//!
//! ```text
//!   planted ──→ collected      收了
//!      ↑   └──→ dropped        不写了（想通了，或者改了方向）
//!      └────────┘              反悔：还能捡回来
//! ```
//!
//! **"不写了"绝不算失败**（语气层的一贯立场）：作者明确说"这条线我不走了"，
//! 检测就不该再报它。所以规则只看 `planted` 那一种——这与"只报告、不催办"是同一条纪律。
//!
//! # 与增值模块的边界
//!
//! 更漂亮的地图 / 时间轴 / 伏笔账本界面归**模块**（模块只读消费核心数据）；
//! 核心这里只提供**数据**与"你自己打开体检时看到的那份静态清单"——
//! 不主动提醒、不弹条、不催办（提醒出口的纪律见规则 9）。

use crate::error::{codes, Error, Result};

/// 一条伏笔现在是什么状态（`foreshadows.state` 的稳定码）。
///
/// `rename_all`：进 JSON 的那一份必须就是稳定码（界面按它挑文案与按钮）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForeshadowState {
    /// 埋着，还没收。
    Planted,
    /// 收了（收在哪一章记在 `collected_node`）。
    Collected,
    /// 明确不写了（**正经结局**，不算失败）。
    Dropped,
}

/// 进 JSON 就用**稳定码本身**。
///
/// 为什么不用 `#[serde(rename_all = "snake_case")]`：变体名与稳定码是两件事
/// （`NoNumber` 的码是 `none`），靠"改蛇形"迟早分家——而且分家时**不报错**，
/// 只是界面上静默对不上。写死成 `as_str()` 就没有第二份口径。
impl serde::Serialize for ForeshadowState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl ForeshadowState {
    /// 全部取值（界面按这个顺序摆；穷举测试拿它对表）。
    pub const ALL: [ForeshadowState; 3] = [
        ForeshadowState::Planted,
        ForeshadowState::Collected,
        ForeshadowState::Dropped,
    ];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            ForeshadowState::Planted => "planted",
            ForeshadowState::Collected => "collected",
            ForeshadowState::Dropped => "dropped",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_foreshadow_state`。
    pub fn parse(s: &str) -> Result<Self> {
        ForeshadowState::ALL
            .into_iter()
            .find(|state| state.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(
                    codes::UNKNOWN_FORESHADOW_STATE,
                    [("value", s.to_string())],
                )
            })
    }

    /// 还能走到哪些态（**唯一真相源**：界面摆哪几个按钮、核心判合法都由它说了算）。
    ///
    /// 三个态都能回到 [`ForeshadowState::Planted`]：收错了、或者改主意又想写了，
    /// 不该被"已经收了"堵死（与延后可取消、静音可撤销同一条思路）。
    pub const fn next_states(self) -> &'static [ForeshadowState] {
        match self {
            ForeshadowState::Planted => {
                &[ForeshadowState::Collected, ForeshadowState::Dropped]
            }
            ForeshadowState::Collected | ForeshadowState::Dropped => {
                &[ForeshadowState::Planted]
            }
        }
    }

    /// 从 `from` 走到 `to` 合法吗（同一个态不算一次迁移）。
    pub fn can_move(from: Self, to: Self) -> bool {
        from != to && from.next_states().contains(&to)
    }
}

/// 一条伏笔。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Foreshadow {
    pub id: i64,
    pub work_id: i64,
    /// 这条线头是什么（作者自己写的一句话）
    pub body: String,
    /// 埋在哪一章；`None` = 没记（那就判不了"隔了多少章"，规则会跳过它）
    pub planted_node: Option<i64>,
    /// 收在哪一章；`None` = 还没收 / 没记
    pub collected_node: Option<i64>,
    pub state: ForeshadowState,
    /// 作者自己写的备注（打算怎么收、为什么先放着……）
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 新建一条伏笔要给的字段（状态一律从"埋着"开始）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewForeshadow {
    pub work_id: i64,
    pub body: String,
    /// 埋在哪一章（界面默认给当前章）；不记也可以，但那样判不了"埋了多久"
    pub planted_node: Option<i64>,
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_codes_round_trip_and_reject_strangers() {
        for state in ForeshadowState::ALL {
            assert_eq!(ForeshadowState::parse(state.as_str()).unwrap(), state);
        }
        let err = ForeshadowState::parse("maybe").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_FORESHADOW_STATE);
    }

    /// 每个态都**从"埋着"到得了**（不留一个只能被手改数据库才出现的态），
    /// 而且每个态都回得到"埋着"（收错了 / 改主意了，不该被堵死）。
    #[test]
    fn every_state_is_reachable_and_has_a_way_back() {
        for state in ForeshadowState::ALL {
            assert!(
                state == ForeshadowState::Planted
                    || ForeshadowState::can_move(ForeshadowState::Planted, state),
                "{state:?} 从「埋着」走不到"
            );
            assert!(
                state == ForeshadowState::Planted
                    || ForeshadowState::can_move(state, ForeshadowState::Planted),
                "{state:?} 回不到「埋着」"
            );
        }
        // 同一个态不算一次迁移；"不写了"与"收了"之间不许直接跳（先回埋着，一次说清一件事）
        assert!(!ForeshadowState::can_move(ForeshadowState::Planted, ForeshadowState::Planted));
        assert!(!ForeshadowState::can_move(
            ForeshadowState::Collected,
            ForeshadowState::Dropped
        ));
    }
}
