//! 半自动登记：模块提候选（只读产出），**作者点确认后由核心落成设定卡**。
//!
//! 三条纪律（与"模块只读核心"一致）：
//! 1. 候选是模块写在自己数据区里的 `candidates.json`，核心只读它；
//! 2. 只有作者点了「落成设定卡」才写核心——写的人是这个命令，不是模块；
//! 3. 落卡时在备注里写明**来源模块与第几章**，以后能追。

use serde::Serialize;
use tauri::State;
use yanmo_core::model::{Attribute, EntityKind, NewEntityCard};
use yanmo_core::store::{ModuleData, Store};

use crate::error::ApiError;
use crate::storage::AppData;

/// 一条候选（模块说的，不是核心判断的）。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleCandidate {
    pub name: String,
    pub kind: String,
    pub chapters: Vec<u64>,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleCandidatesView {
    pub work_id: i64,
    /// 还能落成卡的候选（已经建过的名字不再列）。
    pub items: Vec<ModuleCandidate>,
    /// 模块提过、但书名下已经有同名卡（按声明 + 别名）的条数。
    pub existing: usize,
}

#[tauri::command(rename_all = "snake_case")]
pub fn module_candidates(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<ModuleCandidatesView, ApiError> {
    let (work_id, candidates) = read_candidates(&data, &module_id)?;
    if candidates.is_empty() {
        return Ok(ModuleCandidatesView { work_id, items: Vec::new(), existing: 0 });
    }
    let known = data.with_store(|store| known_names(store, work_id))?;
    let (items, existing) = split_known(candidates, &known);
    Ok(ModuleCandidatesView { work_id, items, existing })
}

/// 把作者选中的候选落成设定卡（核心写）。返回真正新建的张数。
#[tauri::command(rename_all = "snake_case")]
pub fn module_candidates_apply(
    data: State<'_, AppData>,
    module_id: String,
    names: Vec<String>,
) -> Result<usize, ApiError> {
    let (work_id, candidates) = read_candidates(&data, &module_id)?;
    if work_id <= 0 {
        return Ok(0);
    }
    data.with_store(|store: &mut Store| {
        let known = known_names(store, work_id)?;
        let mut created = 0;
        for candidate in candidates {
            if !names.iter().any(|name| name == &candidate.name) {
                continue;
            }
            if known.iter().any(|name| name == &candidate.name) {
                continue;
            }
            // 来源写成**结构化属性**（不写自然语言备注）：模块 id、章节、命中次数各一格，
            // 以后想按来源筛选、导出、统计都读得到，也不占界面文案。
            let chapters = candidate
                .chapters
                .iter()
                .map(|index| (index + 1).to_string())
                .collect::<Vec<_>>()
                .join(",");
            let attributes = vec![
                Attribute { key: "source_module".to_string(), value: module_id.to_string() },
                Attribute { key: "source_chapters".to_string(), value: chapters },
                Attribute { key: "source_hits".to_string(), value: candidate.count.to_string() },
            ];
            store.create_entity_card(
                &NewEntityCard {
                    work_id,
                    kind: if candidate.kind == "person" { EntityKind::Person } else { EntityKind::Setting },
                    name: candidate.name.clone(),
                    aliases: Vec::new(),
                    attributes,
                    note: String::new(),
                },
                "module_candidate",
            )?;
            created += 1;
        }
        Ok(created)
    })
}

/// 挑出"还没有卡"的候选；返回（能落卡的, 已经有卡的条数）。
fn split_known(
    candidates: Vec<ModuleCandidate>,
    known: &[String],
) -> (Vec<ModuleCandidate>, usize) {
    let mut items = Vec::new();
    let mut existing = 0;
    for candidate in candidates {
        if known.iter().any(|name| name == &candidate.name) {
            existing += 1;
        } else {
            items.push(candidate);
        }
    }
    (items, existing)
}

fn read_candidates(
    data: &AppData,
    module_id: &str,
) -> Result<(i64, Vec<ModuleCandidate>), ApiError> {
    let module_data = ModuleData::open(data.data_dir().join("modules.db"))?;
    read_candidates_from(&module_data, module_id)
}

fn read_candidates_from(
    module_data: &ModuleData,
    module_id: &str,
) -> Result<(i64, Vec<ModuleCandidate>), ApiError> {
    let Some(bytes) = module_data.kv_get(module_id, "candidates.json")? else {
        return Ok((0, Vec::new()));
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Ok((0, Vec::new()));
    };
    let work_id = document.get("work_id").and_then(|value| value.as_i64()).unwrap_or(0);
    let items = document
        .get("items")
        .and_then(|value| value.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let name = item.get("name").and_then(|value| value.as_str())?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    Some(ModuleCandidate {
                        name: name.to_string(),
                        kind: item
                            .get("kind")
                            .and_then(|value| value.as_str())
                            .unwrap_or("setting")
                            .to_string(),
                        chapters: item
                            .get("chapters")
                            .and_then(|value| value.as_array())
                            .map(|list| list.iter().filter_map(|value| value.as_u64()).collect())
                            .unwrap_or_default(),
                        count: item.get("count").and_then(|value| value.as_u64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((work_id, items))
}

/// 这本书已经有的名字（声明 + 别名），用来避免重复落卡。
fn known_names(store: &Store, work_id: i64) -> yanmo_core::Result<Vec<String>> {
    let mut names = Vec::new();
    for card in store.entity_cards(work_id, None)? {
        if !card.name.is_empty() {
            names.push(card.name);
        }
        names.extend(card.aliases.into_iter().filter(|alias| !alias.is_empty()));
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<ModuleCandidate> {
        vec![
            ModuleCandidate { name: "雾隐城".into(), kind: "setting".into(), chapters: vec![2], count: 1 },
            ModuleCandidate { name: "青石巷".into(), kind: "setting".into(), chapters: vec![0], count: 3 },
        ]
    }

    #[test]
    fn known_names_are_not_proposed_again() {
        let known = vec!["雾隐城".to_string()];
        let (items, existing) = split_known(candidates(), &known);
        assert_eq!(existing, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "青石巷");
    }

    #[test]
    fn nothing_known_means_everything_stays() {
        let (items, existing) = split_known(candidates(), &[]);
        assert_eq!(existing, 0);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn a_module_without_candidates_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let data = ModuleData::open(ModuleData::path_beside(&dir.path().join("yanmo.db"))).unwrap();
        let (work_id, items) = read_candidates_from(&data, "app.yanmo.bigmap").unwrap();
        assert_eq!(work_id, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn a_candidates_document_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut data = ModuleData::open(ModuleData::path_beside(&dir.path().join("yanmo.db"))).unwrap();
        data.kv_put(
            "app.yanmo.bigmap",
            "candidates.json",
            r#"{"work_id":7,"items":[{"name":"雾隐城","kind":"setting","chapters":[2],"count":1}]}"#
                .as_bytes(),
        )
        .unwrap();
        let (work_id, items) = read_candidates_from(&data, "app.yanmo.bigmap").unwrap();
        assert_eq!(work_id, 7);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "雾隐城");
        assert_eq!(items[0].chapters, vec![2]);
    }
}
