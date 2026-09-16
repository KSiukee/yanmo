//! 创作流碎片（模型）：**同一张表里的几种"作者自己记的东西"**。
//!
//! 事件 / 灵感速记 / 口述段落 / 答案池 / 问题卡**住同一张表**（`fragments`），
//! 靠 `frag_kind` 分家——所以它们天生共享同一套元数据：`source`（怎么记下的）、
//! `created_at`（新鲜度）、`used_count`（新颖度）、`linked`（关联锚点）、
//! `derived_from`（溯源）。**不另起第二张表**是这张表从 v1 起就定下的口径。
//!
//! # 这个文件管什么
//!
//! 只回答两件事：**有哪些种类**（[`FragmentKind`]，闭集，认不出来的当场拒）、
//! **面板读到的一条长什么样**（[`Fragment`]）。怎么读写、怎么留痕在
//! [`crate::store::fragment`]；问题卡与答案各自的语义在它们自己那一族里。
//!
//! # 谁产哪一种（分工写死在这里）
//!
//! - `question` / `answer`：**叩问那条线**产的（选题引擎建卡、作答落答案）；
//! - `idea` / `event` / `dictation`：**创作流面板**随手记的（作者自己写下的）。
//!
//! 两类都不许越界：创作流面板不建问题卡（[`FragmentKind::jotted`] 挡着），
//! 叩问也不往事件池里塞东西。

use crate::error::{codes, Error, Result};

/// 碎片种类（写进 `fragments.frag_kind` 的稳定码；**别改**——老库认的就是它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentKind {
    /// 问题卡（叩问那条线产；创作流面板不建）。
    Question,
    /// 答案（叩问那条线产；创作流面板不建）。
    Answer,
    /// 灵感速记：作者随手记下的一句念头。
    Idea,
    /// 事件：发生了什么（时间线 / 事件池的原料）。
    Event,
    /// 口述段落：语音转写下来、等着归位的段落。
    ///
    /// 存储先到位，**写入者由口述那条链路带**（这一版只有键盘）——
    /// 与 `InputSource::Voice` 同一条纪律：先把位置留出来，别等落地时再改表。
    Dictation,
}

impl FragmentKind {
    /// 全部取值（穷举测试拿它对表；界面按这个顺序摆筛选项）。
    pub const ALL: [FragmentKind; 5] = [
        FragmentKind::Idea,
        FragmentKind::Event,
        FragmentKind::Dictation,
        FragmentKind::Question,
        FragmentKind::Answer,
    ];

    /// **创作流面板记的那几种**（建、删、捞回都只认这一组）。
    pub const JOTTED: [FragmentKind; 3] = [
        FragmentKind::Idea,
        FragmentKind::Event,
        FragmentKind::Dictation,
    ];

    /// 稳定码（写库的那一个）。
    pub const fn as_str(self) -> &'static str {
        match self {
            FragmentKind::Question => "question",
            FragmentKind::Answer => "answer",
            FragmentKind::Idea => "idea",
            FragmentKind::Event => "event",
            FragmentKind::Dictation => "dictation",
        }
    }

    /// 从稳定码解析；认不出来的**报错**，不猜一个近似的。
    pub fn parse(s: &str) -> Result<Self> {
        FragmentKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_FRAGMENT_KIND, [("value", s.to_string())])
            })
    }

    /// 能不能由创作流面板**随手记**。
    ///
    /// 问题卡与答案是叩问那条线产的：它们有自己的状态机与溯源规矩，
    /// 从创作流面板凭空建一张只会长出一张谁也处置不了的孤儿卡。
    pub const fn jotted(self) -> bool {
        matches!(
            self,
            FragmentKind::Idea | FragmentKind::Event | FragmentKind::Dictation
        )
    }
}

/// 创作流面板要读的一条碎片：**跨种类的一份统一形状**。
///
/// 它是"读"的形状，不是库里的行：`linked` 那一列在库里是 JSON 数组文本，
/// 这里已经摊成 `anchors`；库被手改成坏 JSON 时按**空数组**读（这只是展示，
/// 不该让整屏读不出来——问题卡那边要的是"不猜着读"，因为它要拿去算）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Fragment {
    pub id: i64,
    pub work_id: i64,
    pub kind: FragmentKind,
    pub body: String,
    /// 怎么记下的：`typed` / `voice` / `mixed`（与答案同一列、同一闭集）。
    pub source: String,
    /// 关联锚点（`chapter:12` 这种）：记在哪一章下、将来归到哪条线都靠它。
    pub anchors: Vec<String>,
    /// 溯源：从哪张问题卡勾出来的（作者自己随手记的没有）。
    pub derived_from: Option<i64>,
    pub created_at: i64,
}

/// 一种碎片有多少条（面板上那些筛选项的数字，**只数没删的**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct FragmentCount {
    pub kind: FragmentKind,
    pub count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_codes_round_trip_and_reject_strangers() {
        for kind in FragmentKind::ALL {
            assert_eq!(FragmentKind::parse(kind.as_str()).unwrap(), kind);
        }
        let err = FragmentKind::parse("memo").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_FRAGMENT_KIND);
    }

    /// 「谁能随手记」与那两组清单**必须对得上**：两处各说各话就是下一次越界的入口。
    #[test]
    fn jotted_kinds_and_the_jotted_list_agree() {
        let from_all: Vec<FragmentKind> =
            FragmentKind::ALL.into_iter().filter(|k| k.jotted()).collect();
        assert_eq!(from_all, FragmentKind::JOTTED.to_vec());
        for kind in FragmentKind::JOTTED {
            assert!(FragmentKind::parse(kind.as_str()).is_ok());
        }
    }
}
