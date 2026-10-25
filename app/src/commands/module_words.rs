//! 模块词表的读写：**作者在软件里编辑**，不用去手改模块数据区的 JSON。
//!
//! 词表是模块的东西（它自己解释），核心只保证两件事：
//! 1. 写入前**校验形状**（名字非空、分类只有两种、去重、有条数上限）；
//! 2. 两种写法都写成一件事（统一写 `{"name","kind"}`，老模块读纯字符串也认）。

use serde::{Deserialize, Serialize};
use tauri::State;
use yanmo_core::store::{ModuleData, Store};

use crate::error::ApiError;
use crate::storage::AppData;

/// 词表里的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordRow {
    pub name: String,
    /// `person` / `setting`（与核心的实体分类同一个闭集）
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordsView {
    pub items: Vec<WordRow>,
}

/// 一条词表最多多少词（够一个人名/地名表用了；再多就不像"作者手维护的词表"）。
const MAX_WORDS: usize = 500;

#[tauri::command(rename_all = "snake_case")]
pub fn module_words(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<WordsView, ApiError> {
    let module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let Some(bytes) = module_data.kv_get(&module_id, "words.json")? else {
        return Ok(WordsView { items: Vec::new() });
    };
    Ok(WordsView { items: parse_words(&bytes) })
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_words_write(
    data: State<'_, AppData>,
    module_id: String,
    items: Vec<WordRow>,
) -> Result<usize, ApiError> {
    let cleaned = clean_words(items)?;
    let mut module_data = ModuleData::open(ModuleData::path_beside(
        &data.data_dir().join("yanmo.db"),
    ))?;
    let text = serde_json::to_vec(&cleaned)
        .map_err(|error| ApiError::new("shell.module_words_invalid").caused_by(error))?;
    module_data.kv_put(&module_id, "words.json", &text)?;
    Ok(cleaned.len())
}

/// 词表的两种写法都认：纯字符串（按设定算）与 `{"name","kind"}`。
fn parse_words(bytes: &[u8]) -> Vec<WordRow> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|entry| {
                    if let Some(name) = entry.as_str() {
                        let name = name.trim();
                        if name.is_empty() {
                            return None;
                        }
                        return Some(WordRow { name: name.to_string(), kind: "setting".into() });
                    }
                    let name = entry.get("name")?.as_str()?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    let kind = entry.get("kind").and_then(|value| value.as_str()).unwrap_or("setting");
                    Some(WordRow { name: name.to_string(), kind: kind.to_string() })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 写入前的收口：名字去空白、分类只认两种、同名只留第一条、条数有上限。
fn clean_words(items: Vec<WordRow>) -> Result<Vec<WordRow>, ApiError> {
    let mut out: Vec<WordRow> = Vec::new();
    for item in items {
        let name = item.name.trim();
        if name.is_empty() {
            continue;
        }
        let kind = match item.kind.trim() {
            "person" => "person",
            "" | "setting" => "setting",
            _ => return Err(ApiError::new("shell.module_words_invalid")),
        };
        if out.iter().any(|row| row.name == name) {
            continue;
        }
        if out.len() == MAX_WORDS {
            return Err(ApiError::new("shell.module_words_too_many"));
        }
        out.push(WordRow { name: name.to_string(), kind: kind.to_string() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_accepts_both_shapes() {
        let rows = parse_words(br#"["A"," B ",{"name":"C","kind":"person"},{"name":""}]"#);
        assert_eq!(
            rows,
            vec![
                WordRow { name: "A".into(), kind: "setting".into() },
                WordRow { name: "B".into(), kind: "setting".into() },
                WordRow { name: "C".into(), kind: "person".into() },
            ]
        );
    }

    #[test]
    fn writing_cleans_and_rejects_unknown_kind() {
        let cleaned = clean_words(vec![
            WordRow { name: " 甲 ".into(), kind: "person".into() },
            WordRow { name: "甲".into(), kind: "setting".into() },
            WordRow { name: "  ".into(), kind: "person".into() },
        ])
        .unwrap();
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0].name, "甲", "同名只留第一条，名字去空白");
        assert!(clean_words(vec![WordRow { name: "乙".into(), kind: "alien".into() }]).is_err());
    }
}
