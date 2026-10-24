//! 模块提交叩问：**授权凭据 + 提交内容 + 被丢弃的原因**。
//!
//! 叩问只有一个出口（与核心自己的问题同队列、同界面）。模块要走这条出口，必须由**宿主**
//! 出示两件事：它在能力清单里声明过「可否提交叩问」，而且现在是**激活**的。
//! 凭据由宿主按能力清单与生命周期状态填——**核心不信模块的自述**，也不给模块第二条路。
//!
//! # 为什么凭据是一个结构体，而不是让核心自己去查模块表
//!
//! 模块地基（能力清单 / 生命周期 / 宿主进程）还没落地；这条路先把**规则**钉在数据层：
//! 宿主将来从真实注册表读出 `may_ask` / `active` 填进来即可。规则不依赖模块系统怎么实现，
//! 只依赖宿主**如实回答这两个问题**——与「模块交付正文必须署名」同一条纪律。
//!
//! # 来源码
//!
//! 核心自带的问题写 [`CORE_SOURCE`]（`core`）；模块提交的写**模块 id**（按来源静音要用，
//! 见 [`crate::store::question_dispose`]）。模块 id 不许叫 `core`、不许空、不许超长——
//! 否则「来自哪个模块」这一栏就不可信了。

use crate::error::{codes, Error, Result};

/// 核心自带问题的来源码（与模块来源区分；**别改**，老卡认的就是它）。
pub const CORE_SOURCE: &str = "core";

/// 模块 id 的最大长度（字符）。
pub const MAX_MODULE_ID_CHARS: usize = 64;

/// 一条叩问最多带几条依据锚点（多出来的忽略，不报错——它是说明，不是结构）。
pub const MAX_BASIS_ANCHORS: usize = 8;

/// 一条依据锚点的最大长度（字符）。
pub const MAX_BASIS_CHARS: usize = 120;

/// 宿主出示的**授权凭据**：这个模块声明过叩问能力吗、现在激活吗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleQuestionGrant {
    /// 模块 id（稳定标识；会成为问题卡的 `source`）
    pub module_id: String,
    /// 能力清单里勾了「可否提交叩问」
    pub may_ask: bool,
    /// 现在是激活的（未激活＝冻结）
    pub active: bool,
}

impl ModuleQuestionGrant {
    /// 一条就绪的凭据（测试与将来的宿主用）。
    pub fn ready(module_id: &str) -> Self {
        Self { module_id: module_id.to_string(), may_ask: true, active: true }
    }
}

/// 模块能提交的内容：**只有问题本身、重要度与依据**。
///
/// 没有模板键、没有派生来源、没有 `source`——那些由核心按通道规则填：
/// 模块不能替自己盖章，也不能冒充核心或别的模块。
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleQuestion {
    pub work_id: i64,
    pub body: String,
    /// 0~1；越界会被拒（与核心建卡同一条校验）
    pub importance: f64,
    /// 「为什么问」的依据（`chapter:12` / `entity:3` …）：**至少一条**，否则丢弃
    pub linked: Vec<String>,
}

/// 叩问被丢弃的原因（稳定码：进 op-log，也给命令行/测试断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionDropReason {
    /// 能力清单里没勾「可否提交叩问」
    NotDeclared,
    /// 模块未激活（冻结）
    Inactive,
    /// 模块 id 冒充核心（`core`）
    SourceReserved,
    /// 模块 id 空 / 超长 / 字符不合规
    SourceInvalid,
    /// 没有「为什么问」的依据
    NoBasis,
    /// 超过频率上限或还在冷却里
    RateLimited,
}

impl QuestionDropReason {
    /// 全部取值（穷举测试与字典对表用）。
    pub const ALL: [QuestionDropReason; 6] = [
        QuestionDropReason::NotDeclared,
        QuestionDropReason::Inactive,
        QuestionDropReason::SourceReserved,
        QuestionDropReason::SourceInvalid,
        QuestionDropReason::NoBasis,
        QuestionDropReason::RateLimited,
    ];

    /// 稳定码（写进 op-log 的那一个；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            QuestionDropReason::NotDeclared => "not_declared",
            QuestionDropReason::Inactive => "inactive",
            QuestionDropReason::SourceReserved => "source_reserved",
            QuestionDropReason::SourceInvalid => "source_invalid",
            QuestionDropReason::NoBasis => "no_basis",
            QuestionDropReason::RateLimited => "rate_limited",
        }
    }
}

/// 进 JSON 用稳定码本身（与别的码同一手法：不给"改蛇形"留下分家的机会）。
impl serde::Serialize for QuestionDropReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 模块 id 合不合规：**非空、不超长、只含 ASCII 字母数字与 `. _ -`**，且不是核心的来源码。
///
/// 不含空格/全角字符/路径分隔符：这个值会进 `source` 列、界面标签与 op-log，
/// 宽字符既会让"来自 X"读起来像乱码，也会让按来源静音对不上。
/// `core` 也在这里被挡掉（它是保留字；入口会先单独报「冒充核心」这个更具体的原因）。
pub fn is_valid_module_id(module_id: &str) -> bool {
    let value = module_id.trim();
    !value.is_empty()
        && value.chars().count() <= MAX_MODULE_ID_CHARS
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !is_core_source(value)
}

/// 是不是核心的来源码（大小写不敏感：`Core` / `CORE` 都不许被模块占用）。
pub fn is_core_source(source: &str) -> bool {
    source.trim().eq_ignore_ascii_case(CORE_SOURCE)
}

/// 依据锚点：去空白、去空串、去重、截到上限。**一条都不剩就是没依据**。
pub fn clean_basis(linked: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for anchor in linked {
        let value = anchor.trim();
        if value.is_empty() {
            continue;
        }
        let value: String = value.chars().take(MAX_BASIS_CHARS).collect();
        if !out.contains(&value) {
            out.push(value);
        }
        if out.len() == MAX_BASIS_ANCHORS {
            break;
        }
    }
    out
}

/// 交给核心这一侧的**最后一道校验**：模块提交的正文/重要度与核心建卡同一套拒绝理由。
///
/// 放在这里而不是只写在存储层：宿主将来在把模块消息转成 [`ModuleQuestion`] 时，
/// 可以先做一次同样的校验（越界的重要度不必等落库才报）。
pub fn validate_draft(draft: &ModuleQuestion) -> Result<()> {
    if draft.body.trim().is_empty() {
        return Err(Error::invalid(codes::CARD_BODY_EMPTY));
    }
    if !draft.importance.is_finite() || !(0.0..=1.0).contains(&draft.importance) {
        return Err(Error::invalid_with(
            codes::CARD_IMPORTANCE_OUT_OF_RANGE,
            [("value", draft.importance.to_string())],
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_ids_are_narrow_and_bounded() {
        for good in ["mod.map", "map-2", "a_b.c", "Mod9"] {
            assert!(is_valid_module_id(good), "应接受：{good}");
        }
        for bad in ["", "  ", "core", "有中文", "a b", "a/b", &"x".repeat(65)] {
            assert!(!is_valid_module_id(bad), "应拒绝：{bad:?}");
        }
    }

    #[test]
    fn core_cannot_be_used_as_a_module_source() {
        for attempt in ["core", "Core", " CORE "] {
            assert!(is_core_source(attempt), "必须认出冒充：{attempt:?}");
        }
        assert!(!is_core_source("mod.core"));
    }

    #[test]
    fn basis_is_trimmed_deduped_and_bounded() {
        let anchors = vec![
            " chapter:12 ".to_string(),
            String::new(),
            "chapter:12".to_string(),
            "entity:3".to_string(),
        ];
        assert_eq!(clean_basis(&anchors), vec!["chapter:12", "entity:3"]);
        assert!(clean_basis(&["  ".to_string()]).is_empty());

        let many: Vec<String> = (0..20).map(|index| format!("chapter:{index}")).collect();
        assert_eq!(clean_basis(&many).len(), MAX_BASIS_ANCHORS);
    }

    #[test]
    fn draft_validation_matches_the_card_rules() {
        let mut draft = ModuleQuestion {
            work_id: 1,
            body: "要不要给他一个转折点？".to_string(),
            importance: 0.5,
            linked: vec!["chapter:1".to_string()],
        };
        draft.validate_ok();
        draft.importance = 1.5;
        assert_eq!(draft.validate_err(), codes::CARD_IMPORTANCE_OUT_OF_RANGE);
        draft.importance = 0.5;
        draft.body = "   ".to_string();
        assert_eq!(draft.validate_err(), codes::CARD_BODY_EMPTY);
    }

    impl ModuleQuestion {
        fn validate_ok(&self) {
            validate_draft(self).unwrap();
        }
        fn validate_err(&self) -> &'static str {
            validate_draft(self).unwrap_err().code()
        }
    }
}
