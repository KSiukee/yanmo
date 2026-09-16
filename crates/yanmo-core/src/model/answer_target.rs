//! 答案落到哪儿：**正文段落 / 章纲 / 场景卡**——作者答完一句话，它去哪儿由他说了算。
//!
//! 三档的边界（都是"作者自己的字"，机制一个字都不生成）：
//!
//! - `body`：**正文段落**。答完就地插进这一章的正文（或先攒着，一轮问完一起插）；
//! - `outline`：**章纲**——这一章的"一句话"。写进 `nodes.summary`，投稿包的大纲用的就是它
//!   （多条答案合并成一行，见 [`crate::store`] 里落章那一处的口径）；
//! - `scene`：**场景卡**——在这一章下面新建一张 `NodeKind::Scene`，答案就是它的正文。
//!
//! 为什么是闭集而不是随便一个字符串：落点决定**哪张表被写**（正文 / 节点的一句话 / 节点树）。
//! 认不出的取值当场拒绝，绝不猜一个近似的——落错地方比落不下去难查得多。
//!
//! 加一种落点＝在这里加一个取值，别处不用动。

use crate::error::{codes, Error, Result};

/// 一条答案的落点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum AnswerTarget {
    /// 正文段落（默认）。
    Body,
    /// 章纲：这一章的一句话。
    Outline,
    /// 场景卡：这一章下面新建一张。
    Scene,
}

impl AnswerTarget {
    /// 全部取值（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [AnswerTarget; 3] = [AnswerTarget::Body, AnswerTarget::Outline, AnswerTarget::Scene];

    /// 稳定码（进 JSON / 进 op-log；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            AnswerTarget::Body => "body",
            AnswerTarget::Outline => "outline",
            AnswerTarget::Scene => "scene",
        }
    }

    /// 从稳定码解析；认不出的报 `answer.target_unknown`。
    pub fn parse(s: &str) -> Result<Self> {
        AnswerTarget::ALL
            .into_iter()
            .find(|target| target.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::ANSWER_TARGET_UNKNOWN, [("value", s.to_string())])
            })
    }

    /// 从请求里给的取值解析：**空串当正文**（不带这一栏的调用与以前行为一致），
    /// 别的认不出的取值当场拒。
    pub fn from_wire(s: &str) -> Result<Self> {
        if s.trim().is_empty() {
            return Ok(AnswerTarget::Body);
        }
        AnswerTarget::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_codes_round_trip_and_reject_strangers() {
        for target in AnswerTarget::ALL {
            assert_eq!(AnswerTarget::parse(target.as_str()).unwrap(), target);
        }
        assert_eq!(AnswerTarget::from_wire("  ").unwrap(), AnswerTarget::Body, "没给就是正文");
        let err = AnswerTarget::parse("nowhere").unwrap_err();
        assert_eq!(err.code(), codes::ANSWER_TARGET_UNKNOWN);
    }
}
