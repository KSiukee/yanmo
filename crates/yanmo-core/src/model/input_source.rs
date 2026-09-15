//! 输入方式：**文本是文本，怎么打出来的是另一个维度**。
//!
//! 答案（`frag_kind = 'answer'`）与灵感速记（`'idea'`）落在同一个 `source` 列上，
//! 列里存的是这张卡的稳定码：`typed` / `voice` / `mixed`。存的就是原文，不是索引。
//!
//! # 为什么是闭集，而不是"随便一个字符串"
//!
//! 这一列是**创作留痕的原始素材**（"这段话是人打的还是口述的"）：将来按它算口径、
//! 出留痕自证包都要用它。随便放字符串进去，那一栏就再也算不准了——所以认不出来的取值
//! **当场拒绝**，而不是静默收下（静默收下比拒绝坏得多：错的数据已经进了库）。
//!
//! 加一种输入方式＝在这里加一个取值，别处不用动（"只有一处知道有哪些输入方式"）。
//!
//! 注意别与问题卡的 `source` 混了：问题卡那一列的语义是**来源**（`core` / 模块名），
//! 不是输入方式。两列同名不同义，是因为统一表里按 `frag_kind` 分家——
//! 口径写在各自那一族的注释里。

use crate::error::{codes, Error, Result};

/// 这一段文字是怎么产生的（`fragments.source` 的稳定码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum InputSource {
    /// 键盘敲的（含输入法组字）。
    Typed,
    /// 口述转写的（语音链路落地后才有写入者）。
    Voice,
    /// 混着来的：口述之后又用键盘改了，或者边打边说。
    Mixed,
}

impl InputSource {
    /// 全部取值（穷举测试拿它对表）。
    pub const ALL: [InputSource; 3] = [InputSource::Typed, InputSource::Voice, InputSource::Mixed];

    /// 稳定码（写库的那一个；**别改**——老库的 `source` 列认的就是它）。
    pub const fn as_str(self) -> &'static str {
        match self {
            InputSource::Typed => "typed",
            InputSource::Voice => "voice",
            InputSource::Mixed => "mixed",
        }
    }

    /// 从稳定码解析；认不出来的**报错**，不猜一个近似的（猜错就是往留痕里写假数据）。
    pub fn parse(s: &str) -> Result<Self> {
        InputSource::ALL
            .into_iter()
            .find(|source| source.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::INPUT_SOURCE_UNKNOWN, [("value", s.to_string())])
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_codes_round_trip_and_reject_strangers() {
        for source in InputSource::ALL {
            assert_eq!(InputSource::parse(source.as_str()).unwrap(), source);
        }
        let err = InputSource::parse("telepathy").unwrap_err();
        assert_eq!(err.code(), codes::INPUT_SOURCE_UNKNOWN);
    }
}
