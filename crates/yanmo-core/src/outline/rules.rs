//! 四条规则本身：**吃值、吐发现**（不碰库、不产句子）。
//!
//! 每条规则一个小函数，各管自己那一件事；它们只关心"对不上"这个事实——
//! 好赖评价、语气、排序都归别的层。判据只有两条：
//!
//! 1. **同一处出现两种说法**才算冲突（同一个值写两遍不算——那只是重复）；
//! 2. **空的不算说法**（还没填 ≠ 填了个空的），否则"刚建完卡"会满屏假冲突。

use std::collections::{BTreeMap, BTreeSet};

use super::issue::{IssueRule, OutlineIssue};
use crate::model::{EntityCard, SceneFields};

/// 跑一遍规则要的那点数据（**纯值**：读库在 `store::outline_scan`）。
pub struct OutlineData<'a> {
    /// 这本书的设定卡（人物 / 设定，软删的不要）。
    pub cards: &'a [EntityCard],
    /// 这本书的场景卡：`(节点 id, 名字, 四格)`——按树里的顺序。
    pub scenes: &'a [(i64, String, SceneFields)],
}

/// 跑全部规则，按**规则 → 位置**的稳定顺序给（同一份数据两次扫描结果一模一样）。
pub fn scan(data: &OutlineData<'_>) -> Vec<OutlineIssue> {
    let mut issues = Vec::new();
    issues.extend(name_clashes(data.cards));
    issues.extend(attribute_conflicts(data.cards));
    issues.extend(scene_gaps(data.scenes));
    issues
}

/// 一个称呼（名字或别称）在不同的卡上出现 → [`IssueRule::EntityNameClash`]；
/// 同一张卡上同一个称呼登记两遍 → [`IssueRule::EntityNameRepeated`]。
///
/// 名字与别称**一起算**：别称撞车和名字撞车一样糟（作者在正文里喊一声"阿昭"，
/// 两个人回头——这正是称谓冲突）。
fn name_clashes(cards: &[EntityCard]) -> Vec<OutlineIssue> {
    // 有序容器：同一份数据两次跑出来的顺序必须一样（否则界面上的清单会自己跳）
    let mut owners: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut repeated = Vec::new();

    for card in cards {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for token in std::iter::once(card.name.as_str()).chain(card.aliases.iter().map(String::as_str)) {
            let token = token.trim();
            if token.is_empty() {
                continue; // 空的不是称呼（存的时候就该丢，这里再挡一次）
            }
            if !seen.insert(token) {
                repeated.push(OutlineIssue::new(
                    IssueRule::EntityNameRepeated,
                    vec![entity_anchor(card.id)],
                    vec![token.to_string()],
                    [("name", token.to_string())],
                ));
                continue;
            }
            owners.entry(token.to_string()).or_default().push(card.id);
        }
    }

    let mut clashes = Vec::new();
    for (token, mut ids) in owners {
        ids.sort_unstable();
        ids.dedup();
        if ids.len() < 2 {
            continue;
        }
        clashes.push(OutlineIssue::new(
            IssueRule::EntityNameClash,
            ids.iter().map(|id| entity_anchor(*id)).collect(),
            vec![token.clone()],
            [
                ("name", token),
                ("count", ids.len().to_string()),
                // 界面上要说"哪两张"，所以带**名字**（id 在锚点里已经有了，不重复给）
                (
                    // 名字按 `,` 拼（**中性分隔**）：顿号是界面按字典拼的——
                    // 核心不许出现界面文案（这一行的判据是"零文案"，不是"少写几个字"）
                    "names",
                    ids.iter()
                        .filter_map(|id| cards.iter().find(|card| card.id == *id))
                        .map(|card| card.name.clone())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            ],
        ));
    }
    // 同一条规则里按称呼排（读起来顺，也让两次扫描结果一致）
    clashes.extend(repeated);
    clashes
}

/// 同一张卡里同一个属性键给了两个不同的值 → [`IssueRule::EntityAttributeConflict`]。
fn attribute_conflicts(cards: &[EntityCard]) -> Vec<OutlineIssue> {
    let mut issues = Vec::new();
    for card in cards {
        // 键 → 出现过的值（**空值不算说法**：那是"还没填"）
        let mut values: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for attr in &card.attributes {
            let key = attr.key.trim();
            let value = attr.value.trim();
            if key.is_empty() || value.is_empty() {
                continue;
            }
            values.entry(key.to_string()).or_default().insert(value.to_string());
        }
        for (key, found) in values {
            if found.len() < 2 {
                continue; // 同一个值写两遍不算冲突
            }
            issues.push(OutlineIssue::new(
                IssueRule::EntityAttributeConflict,
                vec![entity_anchor(card.id)],
                // 身份带**属性键**：同一张卡的另一个属性以后又冲突了，照样要报
                vec![key.clone()],
                [
                    ("name", card.name.clone()),
                    ("key", key),
                    (
                        "values",
                        found.into_iter().collect::<Vec<_>>().join(" / "),
                    ),
                ],
            ));
        }
    }
    issues
}

/// 场景卡四格缺项 → [`IssueRule::SceneMissingFields`]（缺哪几格写在参数里）。
fn scene_gaps(scenes: &[(i64, String, SceneFields)]) -> Vec<OutlineIssue> {
    let mut issues = Vec::new();
    for (node_id, title, fields) in scenes {
        let missing = fields.missing();
        if missing.is_empty() {
            continue;
        }
        issues.push(OutlineIssue::new(
            IssueRule::SceneMissingFields,
            vec![format!("scene:{node_id}")],
            // 主体就是这一场（锚点已经指明）；缺了哪几格是**状态**，不进身份
            Vec::new(),
            [
                ("title", title.clone()),
                ("count", missing.len().to_string()),
                (
                    "missing",
                    missing
                        .iter()
                        .map(|field| field.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            ],
        ));
    }
    issues
}

/// 定位锚点：与碎片那一套写法一致（`entity:<id>` / `scene:<id>`）。
fn entity_anchor(id: i64) -> String {
    format!("entity:{id}")
}
