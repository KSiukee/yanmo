//! 「什么时候再问我」的**预置档位**：界面给作者的那几个选项。
//!
//! 核心只给**键**（`question.defer.<键>`），话在界面字典里——与其他文案同一条规矩。
//! 档位落在条件模型（[`super::defer`]）上：三档时间、一档"写到这一章"、一档"我自己想起来"。
//!
//! 「写完这一章再问」要一个锚点章：**没给就返回 `None`**——界面不该把一个填不出条件的选项
//! 摆给作者（与其到写库时才报错，不如这一档此刻不可用）。

use super::defer::{DeferCondition, DeferKind, DAY_MS};

/// 界面给作者的那几档「什么时候再问我」——核心只给**键**，话在界面字典里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeferPreset {
    AfterOneDay,
    AfterThreeDays,
    AfterOneWeek,
    WhenChapterWritten,
    OnlyWhenAsked,
}

impl DeferPreset {
    /// 全部档位（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [DeferPreset; 5] = [
        DeferPreset::AfterOneDay,
        DeferPreset::AfterThreeDays,
        DeferPreset::AfterOneWeek,
        DeferPreset::WhenChapterWritten,
        DeferPreset::OnlyWhenAsked,
    ];

    /// 稳定键（界面字典 `question.defer.<键>`；别改）。
    pub const fn key(self) -> &'static str {
        match self {
            DeferPreset::AfterOneDay => "after_one_day",
            DeferPreset::AfterThreeDays => "after_three_days",
            DeferPreset::AfterOneWeek => "after_one_week",
            DeferPreset::WhenChapterWritten => "when_chapter_written",
            DeferPreset::OnlyWhenAsked => "only_when_asked",
        }
    }

    /// 从稳定键解析（界面/命令行按这个键对话）；认不出返回 `None`——
    /// 命令行的"键写错了"是**用法错**，不该跟核心的取值错混在一起。
    pub fn parse(key: &str) -> Option<Self> {
        DeferPreset::ALL.into_iter().find(|preset| preset.key() == key)
    }

    /// 这一档对应哪类条件。
    pub const fn kind(self) -> DeferKind {
        match self {
            DeferPreset::AfterOneDay | DeferPreset::AfterThreeDays | DeferPreset::AfterOneWeek => {
                DeferKind::Time
            }
            DeferPreset::WhenChapterWritten => DeferKind::Written,
            DeferPreset::OnlyWhenAsked => DeferKind::Manual,
        }
    }

    /// 照这一档算出条件。
    ///
    /// 「写完这一章再问」要一个锚点章：**没给就返回 `None`**——界面不该把一个
    /// 填不出条件的选项摆给作者（与其到写库时才报错，不如这一档此刻不可用）。
    pub fn condition(self, now_ms: i64, anchor_node: Option<i64>) -> Option<DeferCondition> {
        match self {
            DeferPreset::AfterOneDay => Some(DeferCondition::after_ms(now_ms + DAY_MS)),
            DeferPreset::AfterThreeDays => Some(DeferCondition::after_ms(now_ms + 3 * DAY_MS)),
            DeferPreset::AfterOneWeek => Some(DeferCondition::after_ms(now_ms + 7 * DAY_MS)),
            DeferPreset::WhenChapterWritten => anchor_node.map(DeferCondition::when_written),
            DeferPreset::OnlyWhenAsked => Some(DeferCondition::manual()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_keys_round_trip_and_reject_strangers() {
        for preset in DeferPreset::ALL {
            assert_eq!(DeferPreset::parse(preset.key()), Some(preset));
        }
        assert_eq!(DeferPreset::parse("someday"), None);
    }

    #[test]
    fn presets_build_conditions_and_never_build_half_ones() {
        let now = 1_000_000_000_000;
        assert_eq!(
            DeferPreset::AfterOneDay.condition(now, None),
            Some(DeferCondition::after_ms(now + DAY_MS))
        );
        assert_eq!(
            DeferPreset::AfterOneWeek.condition(now, None).unwrap().due_at_ms(),
            Some(now + 7 * DAY_MS)
        );
        // 没有锚点章时，「写完这一章再问」这一档**给不出条件**（界面据此把它置灰）
        assert_eq!(DeferPreset::WhenChapterWritten.condition(now, None), None);
        assert_eq!(
            DeferPreset::WhenChapterWritten.condition(now, Some(7)),
            Some(DeferCondition::when_written(7))
        );
        assert_eq!(
            DeferPreset::OnlyWhenAsked.condition(now, None),
            Some(DeferCondition::manual())
        );
    }
}
