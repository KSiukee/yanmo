//! 结构节点（`nodes` 表）——**可变深度树**。

use crate::error::{codes, Error, Result};

/// 新建条目时用什么命名规则（作者在设置里选的**稳定代码**）。
///
/// 号是位置的函数（见 [`crate::numbering`]），这里选的只是**模板长什么样**：
/// - `Arabic`：`第{$N}章` → `第3章`
/// - `Chinese`：`第{$N_ZH}章` → `第三章`
/// - `Padded`：`第{$N:3}章` → `第003章`（网文里常见）
/// - `NoNumber`：**不编号**——标题留给作者自己起（散文 / 随笔 / 文集）
///
/// "没选过"是 `None`（不在这个枚举里）：由作品类型给默认（长篇给号、单篇与文集不编号），
/// 见 [`crate::model::WorkKind::default_naming`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum NamingStyle {
    Arabic,
    Chinese,
    Padded,
    NoNumber,
}

impl NamingStyle {
    /// 界面按这个顺序列（也是"点一下换下一个"的顺序）。
    pub const ALL: [NamingStyle; 4] =
        [NamingStyle::Arabic, NamingStyle::Chinese, NamingStyle::Padded, NamingStyle::NoNumber];

    pub const fn as_str(self) -> &'static str {
        match self {
            NamingStyle::Arabic => "arabic",
            NamingStyle::Chinese => "chinese",
            NamingStyle::Padded => "padded",
            NamingStyle::NoNumber => "none",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        NamingStyle::ALL.into_iter().find(|style| style.as_str() == s)
    }

    /// 计数宏那一段（模板里用）。
    pub const fn counter(self) -> &'static str {
        match self {
            NamingStyle::Arabic => "{$N}",
            NamingStyle::Chinese => "{$N_ZH}",
            NamingStyle::Padded => "{$N:3}",
            NamingStyle::NoNumber => "",
        }
    }
}

/// 节点类型。**"卷""章"只是这里的取值，不是表结构**——
/// 深度不写死，所以单篇文章（零层级）与长篇（卷→章→场景卡）能共用一张表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// 卷
    Volume,
    /// 章
    Chapter,
    /// 节
    Section,
    /// 单篇（零层级作品的根节点即正文）
    Piece,
    /// 场景卡
    Scene,
}

impl NodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            NodeKind::Volume => "volume",
            NodeKind::Chapter => "chapter",
            NodeKind::Section => "section",
            NodeKind::Piece => "piece",
            NodeKind::Scene => "scene",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "volume" => Ok(NodeKind::Volume),
            "chapter" => Ok(NodeKind::Chapter),
            "section" => Ok(NodeKind::Section),
            "piece" => Ok(NodeKind::Piece),
            "scene" => Ok(NodeKind::Scene),
            other => {
                Err(Error::invalid_with(codes::UNKNOWN_NODE_KIND, [("value", other.to_string())]))
            }
        }
    }

    /// 是否承载正文（叶子类节点）。容器节点（卷）不挂正文。
    pub const fn holds_body(self) -> bool {
        matches!(
            self,
            NodeKind::Chapter | NodeKind::Section | NodeKind::Piece | NodeKind::Scene
        )
    }

    /// 能不能容纳下级（目录树据此决定"能不能放进去"）。
    ///
    /// 单篇与场景卡是叶子；卷 / 章 / 节都能收下级——**层级不写死，由节点的类型说了算**。
    pub const fn accepts_children(self) -> bool {
        matches!(
            self,
            NodeKind::Volume | NodeKind::Chapter | NodeKind::Section
        )
    }
}

/// 一个结构节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: i64,
    pub work_id: i64,
    /// 父节点；`None` = 根（零层级作品的唯一根就是这个）。
    pub parent_id: Option<i64>,
    pub kind: NodeKind,
    pub title: String,
    pub sort_order: i64,
    /// 预聚合字数（避免每次扫描正文）。
    pub word_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for k in [
            NodeKind::Volume,
            NodeKind::Chapter,
            NodeKind::Section,
            NodeKind::Piece,
            NodeKind::Scene,
        ] {
            assert_eq!(NodeKind::parse(k.as_str()).unwrap(), k);
        }
    }

    #[test]
    fn only_leaves_hold_body() {
        assert!(!NodeKind::Volume.holds_body());
        assert!(NodeKind::Chapter.holds_body());
        // ★ 单篇也承载正文 —— 这是"零层级文章"能成立的关键
        assert!(NodeKind::Piece.holds_body());
    }

    #[test]
    fn containers_accept_children_and_leaves_do_not() {
        assert!(NodeKind::Volume.accepts_children() && NodeKind::Chapter.accepts_children());
        assert!(!NodeKind::Piece.accepts_children(), "单篇是零层级的根，不该再往下塞");
        assert!(!NodeKind::Scene.accepts_children(), "场景卡是叶子");
    }
}
