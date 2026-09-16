//! 问题模板池：**机制产问题**的那一半——模板 + 槽位。
//!
//! # 为什么模板里没有一个字的中文
//!
//! 核心零文案（这是全工程的硬规矩）：模板只给**键**与**槽位名**（全是英文标识），
//! 句子住在界面字典里（`question.template.<键>`），由界面按语言渲染。
//! 于是同一套模板能出中文 / 日文 / 英文界面，而核心不认识任何一种自然语言；
//! 槽位的取值是**作者数据与数字**（章标题、字数），随句子一起渲染。
//!
//! # 一个模板长什么样
//!
//! 键（稳定标识，进库）、要素类别（哪一类写作要素）、槽位名（界面文案里的占位符）、
//! 基础紧迫度（要素有多"到时候了"）、重要度（这条卡在引力公式里的分量）。
//! 后两个都是 L1 的产品旋钮，集中在这里，好一眼看全、好调。
//!
//! 槽位名必须**全是小写英文与下划线**——界面字典对占位符有同样的机械校验，两处口径一致。

/// 模板要吃哪个要素（决定由谁、在什么时候把候选生出来）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    /// 章节本身：有没有这一章、动笔了没有
    Chapter,
    /// 这一章的计划（每章一句话 / 计划要点）
    Plan,
    /// 字数节奏：长短起伏、是不是该拆
    Rhythm,
    /// 承上启下：上一章刚写完，接下来往哪走
    Continuity,
}

impl ElementKind {
    /// 全部取值（界面按这个顺序分组列；穷举测试拿它对表）。
    pub const ALL: [ElementKind; 4] = [
        ElementKind::Chapter,
        ElementKind::Plan,
        ElementKind::Rhythm,
        ElementKind::Continuity,
    ];
}

/// 一条问题模板。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuestionTemplate {
    /// 稳定键：进库（`fragments.template_key`）、进界面字典；**别改**
    pub key: &'static str,
    pub element: ElementKind,
    /// 界面文案里的占位符（**不含句子**）
    pub slots: &'static [&'static str],
    /// 基础紧迫度 0~1：这类问题"到时候了"的程度
    pub base_urgency: f64,
    /// 重要度 0~1：生出来的卡在引力公式里的分量
    pub importance: f64,
}

/// L1 起步池——只用**已经有的数据**（章节树、每章一句话、字数）就能问出来的问题。
///
/// 池子会长大：智能版会把伏笔超期、五线推进度、灵感碎片引力接进同一个引力公式，
/// 那时加模板即可，引擎一行不用改。
pub const TEMPLATES: &[QuestionTemplate] = &[
    QuestionTemplate {
        key: "chapter.empty_body",
        element: ElementKind::Chapter,
        slots: &["chapter"],
        base_urgency: 0.8,
        importance: 0.7,
    },
    QuestionTemplate {
        key: "chapter.missing_summary",
        element: ElementKind::Plan,
        slots: &["chapter"],
        base_urgency: 0.5,
        importance: 0.5,
    },
    // 开篇两问：**只在开头还没落笔时**才问（见 `generate` 的第 ⑥ 条）。
    // 一本书刚开、第一章还空着的时候，"这一章从哪儿开始"太空，
    // 真正要定的是这两件：谁在看、第一场戏在哪儿。
    QuestionTemplate {
        key: "plan.opening_pov",
        element: ElementKind::Plan,
        slots: &["chapter"],
        base_urgency: 0.85,
        importance: 0.7,
    },
    QuestionTemplate {
        key: "plan.opening_scene",
        element: ElementKind::Plan,
        slots: &["chapter"],
        base_urgency: 0.8,
        importance: 0.7,
    },
    QuestionTemplate {
        key: "rhythm.length_swing",
        element: ElementKind::Rhythm,
        slots: &["count", "chars"],
        base_urgency: 0.4,
        importance: 0.4,
    },
    QuestionTemplate {
        key: "rhythm.chapter_very_long",
        element: ElementKind::Rhythm,
        slots: &["chapter", "chars"],
        base_urgency: 0.5,
        importance: 0.5,
    },
    QuestionTemplate {
        key: "review.recent_chapter",
        element: ElementKind::Continuity,
        slots: &["chapter"],
        base_urgency: 0.7,
        importance: 0.6,
    },
];

/// 认不出模板键时用的紧迫度（**不静默丢卡**：模板池变小了，老卡照样参与排序）。
pub const DEFAULT_URGENCY: f64 = 0.5;

/// 按稳定键找模板。
pub fn template(key: &str) -> Option<&'static QuestionTemplate> {
    TEMPLATES.iter().find(|t| t.key == key)
}

/// 这条卡的模板基础紧迫度；键认不出来时用 [`DEFAULT_URGENCY`]。
pub fn urgency_of(key: &str) -> f64 {
    template(key).map_or(DEFAULT_URGENCY, |t| t.base_urgency)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_pool_is_well_formed() {
        let mut keys = Vec::new();
        for t in TEMPLATES {
            assert!(!t.key.is_empty() && t.key.contains('.'), "模板键要长得像 类别.名字：{}", t.key);
            assert!(t.key.is_ascii(), "模板键只用 ASCII：{}", t.key);
            assert!(!keys.contains(&t.key), "模板键重复：{}", t.key);
            keys.push(t.key);
            assert!(!t.slots.is_empty(), "{} 没有槽位，界面文案就没有可填的东西", t.key);
            for slot in t.slots {
                assert!(
                    slot.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "槽位名只能是小写英文与下划线（与界面字典的校验同口径）：{slot}"
                );
            }
            assert!((0.0..=1.0).contains(&t.base_urgency), "{} 的紧迫度越界", t.key);
            assert!((0.0..=1.0).contains(&t.importance), "{} 的重要度越界", t.key);
        }
        assert!(TEMPLATES.len() >= 5, "起步池太单薄");
    }

    /// 每一类要素都要有模板——否则 [`ElementKind`] 里就有"永远到不了的取值"。
    #[test]
    fn every_element_kind_has_at_least_one_template() {
        for element in ElementKind::ALL {
            assert!(
                TEMPLATES.iter().any(|t| t.element == element),
                "{element:?} 一类模板都没有"
            );
        }
    }
}
