//! 一条「对不上的地方」：**码 + 参数 + 定位锚点**，没有句子。
//!
//! 与错误码（`error_codes.rs`）同一套思路，但**不是错误**：它是**发现**——
//! 界面上那句"「陆文」在两张卡上都登记了"由界面字典渲染（`outline.issue.*`），
//! 核心只给键与取值。这样这一层能单测、能被命令行读、也不把中文塞进数据。

use std::collections::BTreeMap;

/// 哪一条规则报出来的（稳定码；界面按它查字典）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum IssueRule {
    /// 两张设定卡共用一个称呼（名字或别称）——读者会混，作者自己也容易写歪。
    EntityNameClash,
    /// 同一张卡里同一个称呼登记了两遍（手滑；不是冲突，但该顺手清掉）。
    EntityNameRepeated,
    /// 同一张卡里同一个属性键给了两个不同的值（"发色：黑"又"发色：白"）。
    EntityAttributeConflict,
    /// 场景卡的四格缺了（视角 / 目标 / 冲突 / 结果）。
    SceneMissingFields,
}

impl IssueRule {
    /// 全部规则（**字典对表测试拿它穷举**：加一条规则就必须补一条文案）。
    pub const ALL: [IssueRule; 4] = [
        IssueRule::EntityNameClash,
        IssueRule::EntityNameRepeated,
        IssueRule::EntityAttributeConflict,
        IssueRule::SceneMissingFields,
    ];

    /// 稳定码（界面字典键是 `outline.issue.<码>`；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            IssueRule::EntityNameClash => "entity.name_clash",
            IssueRule::EntityNameRepeated => "entity.name_repeated",
            IssueRule::EntityAttributeConflict => "entity.attribute_conflict",
            IssueRule::SceneMissingFields => "scene.missing_fields",
        }
    }

    /// 从稳定码解析（界面把忽略过的那份读回来时要认它；认不出当没忽略）。
    pub fn parse(s: &str) -> Option<Self> {
        IssueRule::ALL.into_iter().find(|rule| rule.as_str() == s)
    }
}

/// 一处对不上的地方。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OutlineIssue {
    pub rule: IssueRule,
    /// 定位锚点（点得动）：`entity:3` / `scene:9`——与碎片那一套锚点写法一致。
    pub anchors: Vec<String>,
    /// **显示**用的取值（名字 / 属性键 / 缺了哪几格……），句子在界面字典里拼。
    pub params: BTreeMap<String, String>,
    /// 这条问题的**身份**：规则 + 位置 + 主体（主体由各规则自己给）。
    ///
    /// 与 `params` 分开是有意的：`params` 里有"状态"（比如缺了哪几格），
    /// 状态一变忽略就失效的话，作者刚填了一格，原来那条又冒出来——那不是忽略，
    /// 那是没记住。身份只带**主体**（哪个称呼撞了车 / 哪张卡的哪个属性）。
    pub identity: Vec<String>,
}

impl OutlineIssue {
    /// 拼一条问题：`anchors` 是定位，`identity` 是主体（见字段说明）。
    pub fn new(
        rule: IssueRule,
        anchors: Vec<String>,
        identity: Vec<String>,
        params: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        Self {
            rule,
            anchors,
            params: params.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            identity,
        }
    }

    /// **忽略标记认它**：同一条问题下一轮必须是同一个串（改一处状态不会换身份）。
    ///
    /// 形状是 `规则|锚点|主体`——三段都用稳定码 / 稳定值，所以它能跨重启、跨设备对上，
    /// 也能被人一眼看懂（排查"为什么这条不报了"时直接看这三个字段）。
    pub fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}",
            self.rule.as_str(),
            self.anchors.join(","),
            self.identity.join(",")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_codes_round_trip_and_reject_strangers() {
        for rule in IssueRule::ALL {
            assert_eq!(IssueRule::parse(rule.as_str()), Some(rule));
        }
        assert_eq!(IssueRule::parse("scene.looks_wrong"), None, "认不出的当没这条规则");
    }

    /// 身份只带主体：**状态变了身份不变**（否则"忽略"会被自己的编辑打断）。
    #[test]
    fn fingerprint_ignores_the_state_but_keeps_the_subject() {
        let make = |missing: &str| {
            OutlineIssue::new(
                IssueRule::SceneMissingFields,
                vec!["scene:9".to_string()],
                Vec::new(), // 主体就是这一场本身（锚点里已经有了）
                [("missing", missing.to_string())],
            )
        };
        assert_eq!(make("pov,goal").fingerprint(), make("pov").fingerprint());

        // 另一张卡上的同名冲突：身份不同（认得出是两回事）
        let other = OutlineIssue::new(
            IssueRule::EntityNameClash,
            vec!["entity:4".to_string()],
            vec!["陆文".to_string()],
            [("name", "陆文".to_string())],
        );
        assert_ne!(other.fingerprint(), make("pov,goal").fingerprint());
        assert_eq!(other.fingerprint(), "entity.name_clash|entity:4|陆文");
    }
}
