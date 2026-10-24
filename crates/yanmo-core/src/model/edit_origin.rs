//! 正文改动的**来源**：这一笔字是怎么进到稿子里的。
//!
//! 这不是"文本是谁想的"，而是"这一次落盘时，壳看见了什么"——只有壳知道
//! 那是敲键盘、整段粘贴、还是某个模块经接口交付的。核心**只记它说的，不猜**：
//! 壳没报就是 `unknown`，绝不默认成"人打的"。
//!
//! # 为什么是闭集
//!
//! 这些码会写进 `op_log` 与来源账，将来还要进创作留痕包。随便放字符串
//! 进去，那一栏就再也算不准了——所以认不出来的取值**当场拒绝**，与
//! [`InputSource`](crate::model::InputSource) 同一条纪律。
//!
//! # 与 `InputSource` 的关系
//!
//! `InputSource` 说的是**一张碎片**是怎么记下的（`typed` / `voice` / `mixed`）；
//! 这里说的是**一笔正文改动**是怎么发生的。两者共用 `typed` 的口径：
//! **键盘敲的（含输入法组字）**——中文写作里组字是打字的一部分，不另立一档，
//! 否则"拼音打一段、中间夹两个标点"就会天天落进 `mixed`，那一档就没有信息了。
//!
//! # 模块交付必须署名
//!
//! 「AI 代写在研墨里是模块的行为」——所以模块递交正文时**必须**带模块 id 与模型名
//! （时间由核心在留痕里记）。没署名的模块交付**核心直接拒收**：拒收比事后猜来源可靠。
//! 这条规矩在数据层，不在界面层——界面可以被绕过，`Store` 不会。

use crate::error::{codes, Error, Result};

/// 这一笔正文改动的来源（写进库的稳定码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditOrigin {
    /// 键盘敲的（含输入法组字）。
    Typed,
    /// 整段粘贴（剪贴板 / 拖放）——只知道"不是打出来的"，来源无法判断。
    Pasted,
    /// 模块经受控接口交付（必须带 [`DeliveryStamp`]）。
    Module,
    /// 口述转写（语音链路落地后才有写入者）。
    Voice,
    /// 软件自己改的：排版清理、作答落章、拆章这类**作者点一下、程序动手**的改动。
    System,
    /// 一次落盘里混了几种来源（壳只能如实说"混了"，不猜各占多少）。
    Mixed,
    /// 壳没报（旧壳 / 命令行 / 恢复与导入这类没有观测者的写入）。
    Unknown,
}

/// 模块交付的署名：**谁、用哪个模型**。时间由核心在留痕里记，不信任调用方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryStamp {
    /// 模块 id（模块清单里的那个稳定标识）
    pub module_id: String,
    /// 模型名（模块自己报的；核心只如实记下）
    pub model: String,
}

/// 一次正文落盘附带的来源说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOrigin {
    pub kind: EditOrigin,
    /// 这一次落盘期间剪贴板里的**字面量**（壳观测到的 plain text 长度之和）。
    ///
    /// 只作证据记在 `op_log` 里，**不参与归属计算**：真正落在正文里多少字，
    /// 由核心按新旧正文比对算（见 `store::source_trace`）。
    pub clipboard_chars: i64,
    /// 模块交付的署名（只有 `Module` 才允许有）。
    pub stamp: Option<DeliveryStamp>,
}

/// 交付署名的长度上限：它是标识，不是一段说明。
const STAMP_MAX_CHARS: usize = 200;

impl EditOrigin {
    /// 全部取值（穷举测试与界面字典对表用）。
    pub const ALL: [EditOrigin; 7] = [
        EditOrigin::Typed,
        EditOrigin::Pasted,
        EditOrigin::Module,
        EditOrigin::Voice,
        EditOrigin::System,
        EditOrigin::Mixed,
        EditOrigin::Unknown,
    ];

    /// 稳定码（写库的那一个；**别改**——老库认的就是它）。
    pub const fn as_str(self) -> &'static str {
        match self {
            EditOrigin::Typed => "typed",
            EditOrigin::Pasted => "pasted",
            EditOrigin::Module => "module",
            EditOrigin::Voice => "voice",
            EditOrigin::System => "system",
            EditOrigin::Mixed => "mixed",
            EditOrigin::Unknown => "unknown",
        }
    }

    /// 从稳定码解析；认不出来的**报错**，不猜一个近似的（猜错就是往留痕里写假数据）。
    pub fn parse(s: &str) -> Result<Self> {
        EditOrigin::ALL
            .into_iter()
            .find(|origin| origin.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_EDIT_ORIGIN, [("value", s.to_string())])
            })
    }
}

/// 进 JSON 就用稳定码本身（与 `InputSource` 同一手法：不给"改蛇形"留下分家的机会）。
impl serde::Serialize for EditOrigin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl WriteOrigin {
    /// 壳什么都没说：**如实记"不知道"**，不冒充成打字。
    pub fn unknown() -> Self {
        Self { kind: EditOrigin::Unknown, clipboard_chars: 0, stamp: None }
    }

    /// 键盘（含输入法）写进来的。
    pub fn typed() -> Self {
        Self { kind: EditOrigin::Typed, clipboard_chars: 0, stamp: None }
    }

    /// 只有来源码、没有别的观测（测试与命令行用）。
    pub fn of(kind: EditOrigin) -> Self {
        Self { kind, clipboard_chars: 0, stamp: None }
    }

    /// 整段粘贴：带上壳观测到的剪贴板字面量（负数按 0 处理——它只是证据，不值得让落盘失败）。
    pub fn pasted(clipboard_chars: i64) -> Self {
        Self { kind: EditOrigin::Pasted, clipboard_chars: clipboard_chars.max(0), stamp: None }
    }

    /// 模块交付：署名必填。模块 id / 模型名去空白后为空 → 拒绝。
    pub fn module(module_id: &str, model: &str, clipboard_chars: i64) -> Result<Self> {
        let stamp = DeliveryStamp::new(module_id, model)?;
        Ok(Self {
            kind: EditOrigin::Module,
            clipboard_chars: clipboard_chars.max(0),
            stamp: Some(stamp),
        })
    }

    /// 落库前的最后一道校验：**模块交付必须带署名，别的来源不许夹带署名**。
    ///
    /// 后者也要拒绝：给一次"打字"盖上模块的章，就是往留痕里写假数据。
    pub fn validate(&self) -> Result<()> {
        match (self.kind, &self.stamp) {
            (EditOrigin::Module, Some(stamp)) => stamp.validate(),
            (EditOrigin::Module, None) => Err(stamp_error("module_id, model")),
            (_, Some(_)) => Err(stamp_error("kind")),
            (_, None) => Ok(()),
        }
    }
}

impl DeliveryStamp {
    fn new(module_id: &str, model: &str) -> Result<Self> {
        let stamp = Self { module_id: module_id.trim().to_string(), model: model.trim().to_string() };
        stamp.validate()?;
        Ok(stamp)
    }

    fn validate(&self) -> Result<()> {
        for (name, value) in [("module_id", &self.module_id), ("model", &self.model)] {
            if value.is_empty() {
                return Err(stamp_error(name));
            }
            if value.chars().count() > STAMP_MAX_CHARS {
                return Err(stamp_error(name));
            }
        }
        Ok(())
    }
}

fn stamp_error(field: &str) -> Error {
    Error::invalid_with(codes::SOURCE_STAMP_INVALID, [("field", field.to_string())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_reject_strangers() {
        for origin in EditOrigin::ALL {
            assert_eq!(EditOrigin::parse(origin.as_str()).unwrap(), origin);
        }
        let err = EditOrigin::parse("telepathy").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_EDIT_ORIGIN);
    }

    #[test]
    fn unknown_is_the_default_and_never_pretends_to_be_typed() {
        assert_eq!(WriteOrigin::unknown().kind, EditOrigin::Unknown);
        assert_eq!(WriteOrigin::unknown().validate().unwrap(), ());
    }

    #[test]
    fn module_delivery_must_carry_both_names() {
        let stamp = WriteOrigin::module("mod.map", "mdl-3", 120).unwrap();
        assert_eq!(stamp.kind, EditOrigin::Module);
        assert_eq!(stamp.clipboard_chars, 120);
        stamp.validate().unwrap();

        assert_eq!(WriteOrigin::module("", "mdl-3", 0).unwrap_err().code(), codes::SOURCE_STAMP_INVALID);
        assert_eq!(WriteOrigin::module("mod.map", "  ", 0).unwrap_err().code(), codes::SOURCE_STAMP_INVALID);
        // 手工拼出来的"模块"没有署名：核心不认
        let bare = WriteOrigin::of(EditOrigin::Module);
        assert_eq!(bare.validate().unwrap_err().code(), codes::SOURCE_STAMP_INVALID);
    }

    #[test]
    fn a_non_module_origin_may_not_carry_a_stamp() {
        let mut origin = WriteOrigin::typed();
        origin.stamp = Some(DeliveryStamp { module_id: "mod.map".into(), model: "mdl-3".into() });
        assert_eq!(origin.validate().unwrap_err().code(), codes::SOURCE_STAMP_INVALID);
    }

    #[test]
    fn clipboard_count_is_an_observation_not_a_reason_to_fail() {
        assert_eq!(WriteOrigin::pasted(-5).clipboard_chars, 0);
        assert_eq!(WriteOrigin::pasted(900).clipboard_chars, 900);
    }
}
