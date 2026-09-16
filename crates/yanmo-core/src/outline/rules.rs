//! 四条规则本身：**吃值、吐发现**（不碰库、不产句子）。
//!
//! 每条规则一个小函数，各管自己那一件事；它们只关心"对不上"这个事实——
//! 好赖评价、语气、排序都归别的层。判据只有两条：
//!
//! 1. **同一处出现两种说法**才算冲突（同一个值写两遍不算——那只是重复）；
//! 2. **空的不算说法**（还没填 ≠ 填了个空的），否则"刚建完卡"会满屏假冲突。

use std::collections::{BTreeMap, BTreeSet};

use super::issue::{IssueRule, OutlineIssue};
use crate::model::{EntityCard, Foreshadow, ForeshadowState, Fragment, SceneFields};

/// 伏笔埋了多少章还没收才算"该看看了"（**章的个数**，不是字数）。
///
/// 20 章：一部长篇（100 章上下）里，一条线头搁了五分之一本书还没动静，
/// 作者多半是真忘了——而那正是"伏笔未回收"要提醒的事。
/// 短篇（十章以内）永远报不出来，这没关系：它本来就不是给短篇用的。
const UNCOLLECTED_CHAPTERS: i64 = 20;

/// 跑一遍规则要的那点数据（**纯值**：读库在 `store::outline_scan`）。
pub struct OutlineData<'a> {
    /// 这本书的设定卡（人物 / 设定，软删的不要）。
    pub cards: &'a [EntityCard],
    /// 这本书的场景卡：`(节点 id, 名字, 四格)`——按树里的顺序。
    pub scenes: &'a [(i64, String, SceneFields)],
    /// 这本书的**章**（阅读顺序的节点 id）——"隔了多少章""第几章"都靠它。
    pub chapters: &'a [i64],
    /// 这本书的伏笔（软删的不要）。
    pub foreshadows: &'a [Foreshadow],
    /// 这本书的事件碎片（只有事件才有故事时间；别的种类由读数据那一层筛掉）。
    pub events: &'a [Fragment],
}

/// 跑全部规则，按**规则 → 位置**的稳定顺序给（同一份数据两次扫描结果一模一样）。
pub fn scan(data: &OutlineData<'_>) -> Vec<OutlineIssue> {
    let mut issues = Vec::new();
    issues.extend(name_clashes(data.cards));
    issues.extend(attribute_conflicts(data.cards));
    issues.extend(scene_gaps(data.scenes));
    issues.extend(uncollected_foreshadows(data.foreshadows, data.chapters));
    issues.extend(out_of_order_events(data.events, data.chapters));
    issues
}

/// 章的**位置账**：节点 id → 第几章（1 起）。不在书里的（锚点指向别的书 / 已删）就没有。
fn chapter_positions(chapters: &[i64]) -> BTreeMap<i64, i64> {
    chapters
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index as i64 + 1))
        .collect()
}

/// 埋了太久还没收的伏笔 → [`IssueRule::ForeshadowUncollected`]。
///
/// 只看 `planted`：**"不写了"是正经结局**（作者说过不走了，就不该再念）；
/// 没记埋在哪一章的也跳过——判不了"隔了多少章"，**绝不猜**。
fn uncollected_foreshadows(
    foreshadows: &[Foreshadow],
    chapters: &[i64],
) -> Vec<OutlineIssue> {
    let positions = chapter_positions(chapters);
    let last = chapters.len() as i64;
    let mut issues = Vec::new();
    for item in foreshadows {
        if item.state != ForeshadowState::Planted {
            continue;
        }
        let Some(planted) = item.planted_node else { continue };
        let Some(at) = positions.get(&planted) else { continue };
        let gap = last - at;
        if gap < UNCOLLECTED_CHAPTERS {
            continue;
        }
        issues.push(OutlineIssue::new(
            IssueRule::ForeshadowUncollected,
            vec![format!("foreshadow:{}", item.id)],
            // 主体就是这一条伏笔（锚点已经指明）；埋在第几章是**状态**，不进身份
            Vec::new(),
            [
                ("body", item.body.clone()),
                ("planted", at.to_string()),
                ("chapters", gap.to_string()),
            ],
        ));
    }
    issues
}

/// 事件的故事时间与它在书里的位置打架 → [`IssueRule::TimelineOutOfOrder`]。
///
/// 判据只有一条：**两边都填了排序值、且都不是倒叙**，后一章的故事时间反而更早。
/// 填得少就报得少——作者没填数字的那条，这里一个字都不猜。
/// 逐条与"到目前为止最晚的那一条"比：一次说清它比谁早（比只跟邻居比更有用）。
fn out_of_order_events(events: &[Fragment], chapters: &[i64]) -> Vec<OutlineIssue> {
    let positions = chapter_positions(chapters);
    // 先在章的顺序上排好（同章内按记录顺序），再走一遍
    let mut dated: Vec<(i64, i64, &Fragment)> = Vec::new();
    for event in events {
        let Some(order) = event.story_order else { continue };
        if event.flashback {
            continue; // 回忆 / 倒叙：后写的章讲更早的事，本来就对
        }
        let Some(at) = event.anchors.iter().find_map(|anchor| {
            anchor.strip_prefix("chapter:").and_then(|id| id.trim().parse::<i64>().ok())
        }) else {
            continue; // 没记在哪一章：说不清先后，不猜
        };
        let Some(position) = positions.get(&at) else { continue };
        dated.push((*position, order, event));
    }
    dated.sort_by_key(|(position, _, event)| (*position, event.id));

    let mut issues = Vec::new();
    let mut latest: Option<(i64, i64, &Fragment)> = None; // (章位置, 故事时间, 事件)
    for (position, order, event) in dated {
        if let Some((_, late_order, late_event)) = latest {
            if order < late_order {
                issues.push(OutlineIssue::new(
                    IssueRule::TimelineOutOfOrder,
                    vec![
                        chapter_anchor(&event.anchors).unwrap_or_else(|| format!("fragment:{}", event.id)),
                        chapter_anchor(&late_event.anchors)
                            .unwrap_or_else(|| format!("fragment:{}", late_event.id)),
                    ],
                    // 身份 = 这一条事件（同一条事件以后又倒置了，还是同一条问题）
                    vec![event.id.to_string()],
                    [
                        ("body", event.body.clone()),
                        ("order", order.to_string()),
                        ("time", event.story_time.clone()),
                        ("earlier_body", late_event.body.clone()),
                        ("earlier_order", late_order.to_string()),
                        ("earlier_time", late_event.story_time.clone()),
                    ],
                ));
            }
        }
        if latest.is_none_or(|(_, late_order, _)| order > late_order) {
            latest = Some((position, order, event));
        }
    }
    issues
}

/// 事件锚点里的章节（`chapter:12` → `chapter:12`）；没有就返回 `None`。
fn chapter_anchor(anchors: &[String]) -> Option<String> {
    anchors
        .iter()
        .find(|anchor| anchor.starts_with("chapter:"))
        .cloned()
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
                    [
                        ("name", token.to_string()),
                        ("kind", card.kind.as_str().to_string()),
                    ],
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
                // 带上**卡的类型**：界面据此把作者带到"人物"或"设定"那一页
                // （这不是文案，是取值；句子仍然全在界面字典里）
                (
                    "kind",
                    ids.iter()
                        .filter_map(|id| cards.iter().find(|card| card.id == *id))
                        .map(|card| card.kind.as_str())
                        .next()
                        .unwrap_or("")
                        .to_string(),
                ),
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
                    ("kind", card.kind.as_str().to_string()),
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
