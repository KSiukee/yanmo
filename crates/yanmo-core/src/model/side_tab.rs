//! 右侧第二栏**露哪一块**：叩问 / 创作流。
//!
//! 那一栏同时住着两条线（[`SideTab::Flow`] 是"机制挑问题问你"、[`SideTab::Creator`]
//! 是"你自己记下的碎片"），栏顶两片页签切——这个取值就是"**上次你停在哪一块**"。
//!
//! 它是**作者的偏好**（存 `appearance`，跟人不跟书），不是数据的属性：
//! 换一块只是换个地方看，一个字节的稿子都不动，也没有任何后台行为跟着变。
//!
//! 为什么要单独成一块（而不是在界面里写两个字符串）：取值只有这一处知道，
//! 写进来不认识的就当场拒（`value.unknown_aside_tab`），不会给界面埋一个
//! "到时候不知道该露哪一块"的空档。

use crate::error::{codes, Error, Result};

/// 第二栏的分区（`appearance.aside_tab` 的稳定代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SideTab {
    /// 叩问：机制挑出问题来问你（**默认**，作者最常待的地方）。
    Flow,
    /// 创作流：你自己记下的灵感 / 事件 / 口述段落。
    Creator,
}

/// 进 JSON 就用**稳定码本身**。
///
/// 为什么不用 `#[serde(rename_all = "snake_case")]`：变体名与稳定码是两件事
/// （`NoNumber` 的码是 `none`），靠"改蛇形"迟早分家——而且分家时**不报错**，
/// 只是界面上静默对不上。写死成 `as_str()` 就没有第二份口径。
impl serde::Serialize for SideTab {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl SideTab {
    /// 全部取值（页签的顺序；穷举测试拿它对表）。
    pub const ALL: [SideTab; 2] = [SideTab::Flow, SideTab::Creator];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            SideTab::Flow => "flow",
            SideTab::Creator => "creator",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_aside_tab`。
    pub fn parse(s: &str) -> Result<Self> {
        SideTab::ALL
            .into_iter()
            .find(|tab| tab.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_ASIDE_TAB, [("value", s.to_string())])
            })
    }
}

impl Default for SideTab {
    /// 默认**叩问**：第一次打开时那一栏先露叩问。
    fn default() -> Self {
        SideTab::Flow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_tab_codes_round_trip_and_reject_strangers() {
        for tab in SideTab::ALL {
            assert_eq!(SideTab::parse(tab.as_str()).unwrap(), tab);
        }
        assert_eq!(SideTab::default(), SideTab::Flow);
        let err = SideTab::parse("timeline").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_ASIDE_TAB);
    }
}
