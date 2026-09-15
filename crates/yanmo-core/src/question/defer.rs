//! 延后条件（纯逻辑）：**延后不是"过会儿再问"，而是要带一个能判的条件**。
//!
//! 为什么非要带条件：一句"延后"如果不带重出条件，那条问题要么永远回不来（成了死路），
//! 要么过一会儿又冒出来（等于没延后）。带上条件之后，"什么时候再问"对作者是**可预期的**，
//! 对系统是**可判定的**。
//!
//! # 三类条件（本版能真判的）
//!
//! | 类型 | 判据 | 界面上的说法（字典） |
//! |---|---|---|
//! | [`DeferKind::Time`] | 到点 | 一天/三天/一周后再问 |
//! | [`DeferKind::Written`] | 锚点子树里出现正文 | 写完这一章再问 / 写到第 X 卷之后再问 |
//! | [`DeferKind::Manual`] | 永不自动重出 | 我自己想起来再问 |
//!
//! 设计里还有两类**现在判不了**、所以不做假实现：人物再次登场（要人物卡）、
//! 伏笔超期后紧迫度上升（要伏笔库）——它们属于叩问的智能版，落地时在这里加一个 kind 即可。
//! 顺带说清一件事：**"事件类"的一半已经天然成立**——延后本身让新颖度随时间回收，
//! 引力会自己回暖，不需要额外写条件。
//!
//! # 靠类型防半截条件
//!
//! [`DeferCondition`] 的字段是**私有**的，只能经三个构造器建出来：
//! "写到某处"却没有锚点、"到点"却没有时刻这类半截条件**根本构造不出来**——
//! 查条件的代码因此不必替调用方兜底（与"目录树不带正文"同一条思路：拿不到就是拿不到）。

use crate::error::{codes, Error, Result};

/// 一天的毫秒数（时间条件与预置档位都用它）。
pub const DAY_MS: i64 = 86_400_000;

/// 条件类型（稳定码，写进库；**别改**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeferKind {
    /// 时间：到点再说。
    Time,
    /// 写到某个节点：锚点子树里出现正文（"这一章写完了"／"进到这一卷了"）。
    Written,
    /// 只有作者：永不自动重出，等他自己捞回（"我自己想起来再问"）。
    Manual,
}

impl DeferKind {
    /// 全部取值（界面按这个顺序列；穷举测试拿它对表）。
    pub const ALL: [DeferKind; 3] = [DeferKind::Time, DeferKind::Written, DeferKind::Manual];

    /// 稳定代码（写库 / 进 JSON；别改）。
    pub const fn as_str(self) -> &'static str {
        match self {
            DeferKind::Time => "time",
            DeferKind::Written => "written",
            DeferKind::Manual => "manual",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_defer_kind`。
    pub fn parse(s: &str) -> Result<Self> {
        DeferKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_DEFER_KIND, [("value", s.to_string())])
            })
    }
}

/// 一条延后条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeferCondition {
    kind: DeferKind,
    due_at_ms: Option<i64>,
    anchor_node: Option<i64>,
}

impl DeferCondition {
    /// 到某个时刻之后再问。
    pub fn after_ms(due_at_ms: i64) -> Self {
        Self { kind: DeferKind::Time, due_at_ms: Some(due_at_ms), anchor_node: None }
    }

    /// 写到这个节点（它自己或它的子孙里出现正文）之后再问。
    pub fn when_written(anchor_node: i64) -> Self {
        Self { kind: DeferKind::Written, due_at_ms: None, anchor_node: Some(anchor_node) }
    }

    /// 不自动重出——等作者自己想起来。
    pub fn manual() -> Self {
        Self { kind: DeferKind::Manual, due_at_ms: None, anchor_node: None }
    }

    pub const fn kind(self) -> DeferKind {
        self.kind
    }

    /// 时间条件的时刻（别的类型为 `None`）。
    pub const fn due_at_ms(self) -> Option<i64> {
        self.due_at_ms
    }

    /// 写到哪（"写到"类条件专用；别的类型为 `None`）。
    pub const fn anchor_node(self) -> Option<i64> {
        self.anchor_node
    }
}

/// 卡上的**章节锚点**：从 `fragments.linked` 那一串里找出 `chapter:<id>`。
///
/// 用途：作者点「写完这一章再问」时，界面不必再问一遍"哪一章"——问题卡自己记着
/// 它是拿哪一章问的（候选生成时就把锚点写进卡里了）。
pub fn chapter_anchor(linked: &str) -> Option<i64> {
    let anchors: Vec<String> = serde_json::from_str(linked).ok()?;
    anchors.iter().find_map(|anchor| {
        anchor.strip_prefix("chapter:").and_then(|id| id.trim().parse::<i64>().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_and_reject_strangers() {
        for kind in DeferKind::ALL {
            assert_eq!(DeferKind::parse(kind.as_str()).unwrap(), kind);
        }
        assert_eq!(
            DeferKind::parse("someday").unwrap_err().code(),
            codes::UNKNOWN_DEFER_KIND
        );
    }

    #[test]
    fn anchors_are_read_off_the_card() {
        assert_eq!(chapter_anchor("[]"), None);
        assert_eq!(chapter_anchor("not json"), None, "坏记录当没有锚点，不炸");
        assert_eq!(chapter_anchor(r#"["rhythm:swing:9"]"#), None);
        assert_eq!(chapter_anchor(r#"["chapter:12","chapter:13"]"#), Some(12));
    }

    #[test]
    fn every_condition_carries_exactly_the_field_its_kind_needs() {
        let time = DeferCondition::after_ms(5);
        assert_eq!((time.kind(), time.due_at_ms(), time.anchor_node()), (DeferKind::Time, Some(5), None));
        let written = DeferCondition::when_written(9);
        assert_eq!(
            (written.kind(), written.due_at_ms(), written.anchor_node()),
            (DeferKind::Written, None, Some(9))
        );
        let manual = DeferCondition::manual();
        assert_eq!((manual.kind(), manual.due_at_ms(), manual.anchor_node()), (DeferKind::Manual, None, None));
    }
}
