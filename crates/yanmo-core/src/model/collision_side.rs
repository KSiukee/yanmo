//! 碰撞的一头：**种类 + 编号**，以及它的规范写法（`spark:12` 这种）。
//!
//! # 为什么住在 model
//!
//! 它是一个**闭集 + 稳定码**，与 [`FragmentKind`](super::FragmentKind) /
//! [`SparkTarget`](super::SparkTarget) / [`SideTab`](super::SideTab) 同一类东西：
//! 进 JSON、进留痕、进 `settings` 里那把"跳过"的记号，写的就是它。
//! 碰撞的**规则**（怎么挑对、关系怎么判）在 [`crate::gravity::collide`]。
//!
//! # 为什么不是裸 id
//!
//! 碎片与大纲元素各住各的表，编号会重号——碎片 3 与伏笔 3 是两样东西。
//! 带上种类，`spark:3` 与 `foreshadow:3` 才是两个不同的对象：去重、"这一对撞过没有"、
//! 跳过的那把记号，全靠这个写法分得清。

use crate::error::{codes, Error, Result};

/// 碰撞的一头是什么（稳定码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollisionKind {
    /// 灵感碎片（作者随手记的那几种）。
    Spark,
    /// 章纲：某一章的"一句话"。
    Plan,
    /// 伏笔（只取还埋着的那种）。
    Foreshadow,
    /// 人物卡 / 设定卡。
    Entity,
}

/// 进 JSON 就用**稳定码本身**（同一个理由见 [`FragmentKind`](super::FragmentKind)）。
impl serde::Serialize for CollisionKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl CollisionKind {
    /// 全部取值（穷举测试与界面对表用）。
    pub const ALL: [CollisionKind; 4] = [
        CollisionKind::Spark,
        CollisionKind::Plan,
        CollisionKind::Foreshadow,
        CollisionKind::Entity,
    ];

    /// 稳定码（**别改**：留痕与"跳过"的记号里写的就是它）。
    pub const fn as_str(self) -> &'static str {
        match self {
            CollisionKind::Spark => "spark",
            CollisionKind::Plan => "plan",
            CollisionKind::Foreshadow => "foreshadow",
            CollisionKind::Entity => "entity",
        }
    }

    /// 排序用的序号——**规范写法**（哪一头在前）靠它定，同一条组合永远同一个写法。
    pub const fn rank(self) -> u8 {
        match self {
            CollisionKind::Spark => 0,
            CollisionKind::Plan => 1,
            CollisionKind::Foreshadow => 2,
            CollisionKind::Entity => 3,
        }
    }

    /// 从稳定码解析；认不出的当场拒（不猜一个近似的）。
    pub fn parse(s: &str) -> Result<Self> {
        CollisionKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::COLLISION_SIDE_UNKNOWN, [("value", s.to_string())])
            })
    }
}

/// 碰撞的一头：**种类 + 它在自己那张表里的 id**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CollisionSide {
    pub kind: CollisionKind,
    pub id: i64,
}

impl CollisionSide {
    pub const fn new(kind: CollisionKind, id: i64) -> Self {
        Self { kind, id }
    }

    /// 规范写法 `kind:id`（**别改**：库里碰撞卡的溯源锚点、跳过记号都是它）。
    pub fn write(self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }

    /// 从 `kind:id` 解析；认不出的当场拒。
    pub fn parse(text: &str) -> Result<Self> {
        let (kind, id) = text.split_once(':').ok_or_else(|| unknown_side(text))?;
        let kind = CollisionKind::parse(kind.trim())?;
        let id = id
            .trim()
            .parse::<i64>()
            .map_err(|_| unknown_side(text))?;
        Ok(Self::new(kind, id))
    }

    /// 排序键：先按种类（碎片在最前），再按 id。
    pub fn rank(self) -> (u8, i64) {
        (self.kind.rank(), self.id)
    }
}

fn unknown_side(value: &str) -> Error {
    Error::invalid_with(codes::COLLISION_SIDE_UNKNOWN, [("value", value.to_string())])
}

/// 两头的**规范写法**（小的在前：先按种类，再按 id）。
pub fn canonical_order(a: CollisionSide, b: CollisionSide) -> (CollisionSide, CollisionSide) {
    if a.rank() <= b.rank() {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_codes_round_trip_and_reject_strangers() {
        for kind in CollisionKind::ALL {
            let one = CollisionSide::new(kind, 12);
            assert_eq!(CollisionSide::parse(&one.write()).unwrap(), one);
        }
        for bad in ["mood:3", "spark:x", "nope", ":3", "spark:"] {
            assert_eq!(
                CollisionSide::parse(bad).unwrap_err().code(),
                codes::COLLISION_SIDE_UNKNOWN,
                "{bad} 该被拒"
            );
        }
    }

    /// 碎片与元素各住各的表：id 重号也是两个对象（规范写法带种类）。
    #[test]
    fn the_same_id_in_two_tables_is_two_objects() {
        let a = CollisionSide::new(CollisionKind::Spark, 3);
        let b = CollisionSide::new(CollisionKind::Foreshadow, 3);
        assert_ne!(a, b);
        assert_eq!(canonical_order(b, a), (a, b), "碎片那一头排在前面");
        assert_eq!(canonical_order(a, b), (a, b));
        assert_eq!(canonical_order(b, b), (b, b));
    }

    /// 种类码与界面字典一一对上（漏一个就是屏幕上露 `collide.side.xxx`）。
    #[test]
    fn every_kind_has_a_stable_code() {
        let codes: Vec<&str> = CollisionKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(codes, vec!["spark", "plan", "foreshadow", "entity"]);
        for kind in CollisionKind::ALL {
            assert_eq!(CollisionKind::parse(kind.as_str()).unwrap(), kind);
        }
    }
}
