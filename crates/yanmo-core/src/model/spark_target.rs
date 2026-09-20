//! 采纳一条灵感时，把它**送到哪儿**——闭集，认不出的当场拒。
//!
//! # 为什么与 [`AnswerTarget`]（答案的落点）是两个闭集
//!
//! 三档是重合的（正文段落 / 章纲 / 场景卡——都是"这一章的那几处"），但**灵感多一档**
//! `event`（把它升成一条**事件**，进事件池），而答案没有这一档（答出来的那句话不是
//! "发生了什么"）。硬挤进一个枚举，两边就都得带着对方的语义走路。
//!
//! **落地的实现两边共用**：章纲走 `store::node_edit::set_summary_in`、场景卡走
//! `create_node_in` + `content::put_body`，正文那一段字仍然由壳内的编辑会话插入
//! （见 [`AnswerTarget`] 那段说明：击键级的正文写入不能过边界）。不共用的只是"有哪几档"。
//!
//! 加一种落点＝在这里加一个取值，别处不用动。

use crate::error::{codes, Error, Result};
use crate::model::AnswerTarget;

/// 一条灵感的落点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SparkTarget {
    /// 正文段落（默认）：字由界面插进编辑会话，核心只做账。
    Body,
    /// 章纲：合到这一章的"一句话"上（原有的在前，用「；」接）。
    Outline,
    /// 场景卡：在这一章下面新建一张，灵感就是它的正文。
    Scene,
    /// 事件池：新建一条**事件**碎片（原文不动），溯源指回这条灵感。
    Event,
}

/// 进 JSON 就用**稳定码本身**（同一个理由见 [`crate::model::FragmentKind`]）。
impl serde::Serialize for SparkTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl SparkTarget {
    /// 全部取值（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [SparkTarget; 4] = [
        SparkTarget::Body,
        SparkTarget::Outline,
        SparkTarget::Scene,
        SparkTarget::Event,
    ];

    /// 稳定码（进 JSON / 进 op-log；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            SparkTarget::Body => "body",
            SparkTarget::Outline => "outline",
            SparkTarget::Scene => "scene",
            SparkTarget::Event => "event",
        }
    }

    /// 落到这一档要不要**先指明是哪一章**（章纲与场景卡挂在章上；正文与事件不挂）。
    pub const fn needs_chapter(self) -> bool {
        matches!(self, SparkTarget::Outline | SparkTarget::Scene)
    }

    /// 三档重合的那一半：交给答案那套落点（写章纲 / 建场景卡就这一条路）。
    /// `event` 没有对应物——它只属于灵感这条线。
    pub const fn structural(self) -> Option<AnswerTarget> {
        match self {
            SparkTarget::Body => Some(AnswerTarget::Body),
            SparkTarget::Outline => Some(AnswerTarget::Outline),
            SparkTarget::Scene => Some(AnswerTarget::Scene),
            SparkTarget::Event => None,
        }
    }

    /// 从稳定码解析；认不出的报 `spark.target_unknown`。
    pub fn parse(s: &str) -> Result<Self> {
        SparkTarget::ALL
            .into_iter()
            .find(|target| target.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::SPARK_TARGET_UNKNOWN, [("value", s.to_string())])
            })
    }

    /// 从请求里给的取值解析：**空串当正文**（与答案那条路同一条口径），别的认不出的当场拒。
    pub fn from_wire(s: &str) -> Result<Self> {
        if s.trim().is_empty() {
            return Ok(SparkTarget::Body);
        }
        SparkTarget::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_codes_round_trip_and_reject_strangers() {
        for target in SparkTarget::ALL {
            assert_eq!(SparkTarget::parse(target.as_str()).unwrap(), target);
        }
        assert_eq!(SparkTarget::from_wire(" ").unwrap(), SparkTarget::Body, "没给就是正文");
        let err = SparkTarget::parse("trash").unwrap_err();
        assert_eq!(err.code(), codes::SPARK_TARGET_UNKNOWN);
    }

    /// 与答案那边重合的三档**必须真的对得上**：两处各说各话，将来就会有一档悄悄落偏。
    #[test]
    fn overlapping_targets_match_the_answer_set() {
        let structural: Vec<AnswerTarget> =
            SparkTarget::ALL.into_iter().filter_map(|t| t.structural()).collect();
        assert_eq!(structural, AnswerTarget::ALL.to_vec());
        assert_eq!(SparkTarget::Event.structural(), None, "事件池只属于灵感这条线");
        assert!(!SparkTarget::Event.needs_chapter());
        assert!(SparkTarget::Outline.needs_chapter() && SparkTarget::Scene.needs_chapter());
    }
}
