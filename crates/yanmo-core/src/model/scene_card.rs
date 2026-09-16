//! 场景卡的四格：**视角 / 目标 / 冲突 / 结果**。
//!
//! 场景卡本来就是目录树上一个节点（`NodeKind::Scene`，名字与正文照旧），
//! 这里补的是它**结构化的那四格**——大纲冲突检测要问"这张卡缺项没有"，
//! 靠正文里的自由文字是问不出结论的，所以四格单独存（`scene_cards` 卫星表）。
//!
//! 三条分寸：
//!
//! 1. **没填就是空串，不猜**：缺项检测只看"修剪后是不是空的"，不替作者判断"这句算不算目标"；
//! 2. **缺项只报告、绝不代写**：那是作者的东西（与"AI 不写正文"同一条纪律）；
//! 3. **四格是主动场景那一套**：被动场景那三格（情感反应 / 困境 / 决定）等真机反馈再加——
//!    先做一套真的，比一次摆七格没人填强。

use crate::error::{codes, Error, Result};

/// 四格里的哪一格（稳定码：界面按它摆输入框，问题清单按它说"缺了哪一格"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneField {
    /// 视角：这一场从谁眼里看出去。
    Pov,
    /// 目标：这一场里，视角人物想要什么。
    Goal,
    /// 冲突：挡在目标前面的东西。
    Conflict,
    /// 结果：这一场结束时落到了哪儿（成功 / 失败 / 更糟）。
    Outcome,
}

/// 进 JSON 就用**稳定码本身**。
///
/// 为什么不用 `#[serde(rename_all = "snake_case")]`：变体名与稳定码是两件事
/// （`NoNumber` 的码是 `none`），靠"改蛇形"迟早分家——而且分家时**不报错**，
/// 只是界面上静默对不上。写死成 `as_str()` 就没有第二份口径。
impl serde::Serialize for SceneField {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl SceneField {
    /// 全部四格（界面的顺序；穷举测试拿它对表）。
    pub const ALL: [SceneField; 4] = [
        SceneField::Pov,
        SceneField::Goal,
        SceneField::Conflict,
        SceneField::Outcome,
    ];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            SceneField::Pov => "pov",
            SceneField::Goal => "goal",
            SceneField::Conflict => "conflict",
            SceneField::Outcome => "outcome",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_scene_field`。
    pub fn parse(s: &str) -> Result<Self> {
        SceneField::ALL
            .into_iter()
            .find(|field| field.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_SCENE_FIELD, [("value", s.to_string())])
            })
    }
}

/// 一张场景卡的四格。**库里没有这一行＝四格都还没填**（读出来就是四个空串）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SceneFields {
    pub node_id: i64,
    pub pov: String,
    pub goal: String,
    pub conflict: String,
    pub outcome: String,
}

impl SceneFields {
    /// 空的那一份（"这张卡还没填过"）。
    pub fn empty(node_id: i64) -> Self {
        Self {
            node_id,
            pov: String::new(),
            goal: String::new(),
            conflict: String::new(),
            outcome: String::new(),
        }
    }

    /// 取某一格。
    pub fn get(&self, field: SceneField) -> &str {
        match field {
            SceneField::Pov => &self.pov,
            SceneField::Goal => &self.goal,
            SceneField::Conflict => &self.conflict,
            SceneField::Outcome => &self.outcome,
        }
    }

    /// 还空着的格子（按 [`SceneField::ALL`] 的顺序）。
    ///
    /// **判据只有一条**：修剪后是不是空的。不替作者判断"这句话算不算一个目标"——
    /// 那是他自己的事，这里只回答"填没填"。
    pub fn missing(&self) -> Vec<SceneField> {
        SceneField::ALL
            .into_iter()
            .filter(|field| self.get(*field).trim().is_empty())
            .collect()
    }

    /// 四格都填了。
    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_codes_round_trip_and_reject_strangers() {
        for field in SceneField::ALL {
            assert_eq!(SceneField::parse(field.as_str()).unwrap(), field);
        }
        let err = SceneField::parse("mood").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_SCENE_FIELD);
    }

    #[test]
    fn missing_only_counts_the_blank_ones() {
        let mut fields = SceneFields::empty(7);
        assert_eq!(fields.missing(), SceneField::ALL.to_vec(), "一行都没填：四格都缺");
        assert!(!fields.is_complete());

        fields.pov = "  林昭  ".to_string();
        fields.goal = "拿到账本".to_string();
        // 只有空白也算没填
        fields.conflict = "  \n ".to_string();
        assert_eq!(fields.missing(), vec![SceneField::Conflict, SceneField::Outcome]);
        assert_eq!(fields.get(SceneField::Pov), "  林昭  ", "原样读出来，不在这里修剪");
    }
}
