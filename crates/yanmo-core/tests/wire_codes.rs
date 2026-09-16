//! 上线那一份的**稳定码守卫**：进 JSON 的取值，必须与 `as_str()` 一模一样。
//!
//! 为什么专门钉这一条：界面上每一处比对（筛选项、徽标、状态按钮）认的都是
//! **稳定码字符串**；枚举忘了 `#[serde(rename_all = "snake_case")]` 就会序列化成
//! `Person` / `Planted` 这种"名字"，而那**不报错**——只是界面上默默地一句都对不上
//! （选项点了没反应、种类显示成英文）。这一条只要机械跑一遍就永远抓得到。
//!
//! 新增一个跨界枚举时，把它加进 [`enums`] 那一串即可。

use serde::Serialize;

use yanmo_core::model::{
    AnswerTarget, ChapterNumbering, EntityKind, ForeshadowState, FragmentKind, InputSource,
    NamingStyle, NodeKind, QuestionState, QuestionTone, SceneField, SideTab,
};
use yanmo_core::question::{DeferKind, DeferPreset};
use yanmo_core::text::WordCaliber;
use yanmo_core::typeset::QuoteStyle;

/// 断言：序列化出来正好是 `"稳定码"`（不带别的形状）。
fn same_code<T: Serialize + std::fmt::Debug>(value: T, code: &str) {
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        format!("\"{code}\""),
        "{value:?} 的序列化形状与稳定码不一致——界面按稳定码比对，会静默对不上"
    );
}

#[test]
fn every_wire_enum_serializes_as_its_stable_code() {
    for kind in FragmentKind::ALL {
        same_code(kind, kind.as_str());
    }
    for kind in EntityKind::ALL {
        same_code(kind, kind.as_str());
    }
    for field in SceneField::ALL {
        same_code(field, field.as_str());
    }
    for state in ForeshadowState::ALL {
        same_code(state, state.as_str());
    }
    for tab in SideTab::ALL {
        same_code(tab, tab.as_str());
    }
    for source in InputSource::ALL {
        same_code(source, source.as_str());
    }
    for state in QuestionState::ALL {
        same_code(state, state.as_str());
    }
    for tone in QuestionTone::ALL {
        same_code(tone, tone.as_str());
    }
    for style in NamingStyle::ALL {
        same_code(style, style.as_str());
    }
    for numbering in ChapterNumbering::ALL {
        same_code(numbering, numbering.as_str());
    }
    for target in AnswerTarget::ALL {
        same_code(target, target.as_str());
    }
    for caliber in WordCaliber::ALL {
        same_code(caliber, caliber.as_str());
    }
    for kind in DeferKind::ALL {
        same_code(kind, kind.as_str());
    }
    for preset in DeferPreset::ALL {
        // 它的码叫 `key()`（顺带当界面字典键用），与 `as_str()` 是同一个意思
        same_code(preset, preset.key());
    }
    // `ElementKind`（问题要素那一类）没有稳定码、也不单独上线（它只出现在候选草稿里，
    // 界面认的是 `QuestionDraft.element` 那个字符串）——所以不进这一串。

    // 引号风格没有 `ALL`（两种，固定），逐个列
    for style in [QuoteStyle::Curly, QuoteStyle::Corner] {
        same_code(style, style.as_str());
    }

    // `NodeKind` 上线了（大纲表那一行按类型分行摆）——它没有 `ALL`，逐个列
    for node in [
        NodeKind::Volume,
        NodeKind::Chapter,
        NodeKind::Section,
        NodeKind::Piece,
        NodeKind::Scene,
    ] {
        same_code(node, node.as_str());
    }
    // `WorkKind` / `WorkLanguage` **仍不在这一串里**：它们没有实现 `Serialize`
    // （进 JSON 时一律由调用方取 `as_str()`），所以不存在"序列化形状跑偏"这回事——
    // 它们的稳定码由各自模块的单测与 DDL / 成稿导出那边的对表钉着。
}
