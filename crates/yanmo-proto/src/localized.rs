//! 清单里的**按语言的文本**（名字 / 说明）与语言码规则。
//!
//! 为什么要按语言：权限提示与模块名必须由**宿主**渲染并本地化（模块绝不能自述权限），
//! 所以清单给的是"每种语言怎么说"，不是一段写死的英文。缺省语言由清单声明一次
//! （`default_language`），名字与说明共用——两处各写一个默认语言迟早会分家。
//!
//! 语言码按 BCP-47 的一个窄子集：主语言 2~3 个小写字母，后面每段 2~8 个字母数字
//! （`zh`、`zh-Hans`、`pt-BR`、`ja`）。**不认**脚本以外的扩展（`-u-` 之类）——
//! 现在用不上，收窄比猜错好。

use std::collections::BTreeMap;

use crate::error::ManifestError;

/// 语言码合不合规（见模块文档的窄子集）。
pub fn is_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let Some(primary) = parts.next() else { return false };
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    parts.all(|part| {
        (2..=8).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// 取一段本地化文本：先精确匹配语言，再退到同主语言（`zh-Hans` → `zh`），
/// 再退到默认语言，最后退到字典里第一条（永远有字可显示，不返回 `None`）。
pub fn localized_text<'a>(
    translations: &'a BTreeMap<String, String>,
    default_language: &str,
    locale: &str,
) -> &'a str {
    if let Some(text) = translations.get(locale) {
        return text;
    }
    let primary = locale.split('-').next().unwrap_or(locale);
    if let Some((_, text)) = translations
        .iter()
        .find(|(tag, _)| tag.split('-').next() == Some(primary))
    {
        return text;
    }
    translations
        .get(default_language)
        .or_else(|| translations.values().next())
        .map(String::as_str)
        .unwrap_or("")
}

/// 校验一份本地化字典：**至少一条、必须含默认语言、语言码与长度都合规矩**。
///
/// `code` 是失败时给出去的稳定码（名字与说明用不同的码，界面能说清是哪一格）。
pub(crate) fn validate_localized(
    translations: &BTreeMap<String, String>,
    default_language: &str,
    field: &'static str,
    code: &'static str,
    max_chars: usize,
) -> Result<(), ManifestError> {
    if !is_language_tag(default_language) {
        return Err(ManifestError::new(code, field, "default_language is not a valid language tag"));
    }
    if translations.is_empty() {
        return Err(ManifestError::new(code, field, "at least one language is required"));
    }
    if !translations.contains_key(default_language) {
        return Err(ManifestError::new(
            code,
            field,
            "default_language must appear in the translations",
        ));
    }
    for (tag, text) in translations {
        if !is_language_tag(tag) {
            return Err(ManifestError::new(code, field, format!("unknown language tag: {tag}")));
        }
        if text.trim().is_empty() {
            return Err(ManifestError::new(code, field, format!("empty text for {tag}")));
        }
        if text.chars().count() > max_chars {
            return Err(ManifestError::new(
                code,
                field,
                format!("text for {tag} is longer than {max_chars} characters"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| ((*key).to_string(), (*value).to_string())).collect()
    }

    #[test]
    fn language_tags_are_a_narrow_bcp47_subset() {
        for good in ["zh", "zh-Hans", "en", "pt-BR", "ja"] {
            assert!(is_language_tag(good), "应接受：{good}");
        }
        for bad in ["", "z", "zh-hans-", "zh--Hans", "zh_CN", "有中文", "zh-Hans-123456789"] {
            assert!(!is_language_tag(bad), "应拒绝：{bad:?}");
        }
    }

    #[test]
    fn lookup_falls_back_to_primary_then_default() {
        let map = text(&[("zh-Hans", "地图"), ("en", "Map")]);
        assert_eq!(localized_text(&map, "zh-Hans", "zh-Hans"), "地图");
        assert_eq!(localized_text(&map, "zh-Hans", "zh-Hant"), "地图");
        assert_eq!(localized_text(&map, "zh-Hans", "en-US"), "Map");
        assert_eq!(localized_text(&map, "zh-Hans", "fr"), "地图");
        assert_eq!(localized_text(&text(&[]), "zh-Hans", "fr"), "");
    }

    #[test]
    fn validation_requires_the_declared_default() {
        let missing = text(&[("en", "Map")]);
        let error = validate_localized(&missing, "zh-Hans", "name", ManifestError::NAME, 20).unwrap_err();
        assert_eq!(error.code, ManifestError::NAME);

        let ok = text(&[("zh-Hans", "地图"), ("en", "Map")]);
        assert!(validate_localized(&ok, "zh-Hans", "name", ManifestError::NAME, 20).is_ok());

        let empty = text(&[("zh-Hans", "  ")]);
        assert_eq!(
            validate_localized(&empty, "zh-Hans", "name", ManifestError::NAME, 20).unwrap_err().code,
            ManifestError::NAME
        );
    }
}
