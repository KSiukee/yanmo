//! 设定卡：**一个人、一个地方、一件东西**——它叫什么、还有哪些叫法、身上有哪些设定。
//!
//! 它是大纲冲突检测的**数据源之一**（另一份是场景卡的四格）：称谓撞车、属性自相矛盾
//! 这两类问题只有在"设定被结构化地记下来"之后才判得动。靠正文自由文字判，
//! 就要实体抽取——那是另一件事，这一版不做。
//!
//! # 为什么另起一张表，不塞进碎片统一表
//!
//! 碎片（`fragments`）是**素材**：一句话、会被"用掉"、会随时间冷却降权；
//! 设定卡是**实体**：它有稳定的身份（名字 / 别称）、有键值属性、不该因为"用过一次"就降权。
//! 两者混在一张表里，碎片那套引力语义（`used_count` / `importance`）就会污染设定——
//! 而"谁是实体、谁是素材"这条线一旦糊了，后面所有按碎片算的东西都会算歪。
//!
//! # 与问题卡的区别
//!
//! 问题卡有自己的六态生命周期（待问 → 已问 → …），设定卡没有：它就在那儿，
//! 改了就是改了（留痕在 op-log）。所以这里只有 `deleted_at` 这一种"不在"。

use crate::error::{codes, Error, Result};

/// 设定卡是哪一类（`entity_cards.card_kind` 的稳定码）。
///
/// `rename_all`：**进 JSON 的那一份必须就是稳定码**（界面按稳定码比对；
/// 漏了它就会序列化成 `Person`，界面上一句"人物"都认不出来）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    /// 人物。
    Person,
    /// 设定：地点 / 物品 / 组织 / 概念——它们只需要"名字 + 属性"，与人物同一套形状。
    Setting,
}

/// 进 JSON 就用**稳定码本身**。
///
/// 为什么不用 `#[serde(rename_all = "snake_case")]`：变体名与稳定码是两件事
/// （`NoNumber` 的码是 `none`），靠"改蛇形"迟早分家——而且分家时**不报错**，
/// 只是界面上静默对不上。写死成 `as_str()` 就没有第二份口径。
impl serde::Serialize for EntityKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl EntityKind {
    /// 全部取值（界面按这个顺序摆；穷举测试拿它对表）。
    pub const ALL: [EntityKind; 2] = [EntityKind::Person, EntityKind::Setting];

    /// 稳定码（进库 / 进 JSON；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            EntityKind::Person => "person",
            EntityKind::Setting => "setting",
        }
    }

    /// 从稳定码解析；认不出的报 `value.unknown_entity_kind`。
    pub fn parse(s: &str) -> Result<Self> {
        EntityKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_ENTITY_KIND, [("value", s.to_string())])
            })
    }
}

/// 一条属性：键 + 值（**都是作者自己写的字**，核心不认识它们的内容）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Attribute {
    pub key: String,
    pub value: String,
}

/// 一张设定卡。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EntityCard {
    pub id: i64,
    pub work_id: i64,
    pub kind: EntityKind,
    /// 正式名（**称谓冲突认它**：两张卡不许共用一个名字）。
    pub name: String,
    /// 别的叫法：字 / 号 / 绰号 / 小名（作者用逗号顿号分隔着录，核心只存成一串）。
    pub aliases: Vec<String>,
    /// 属性键值（发色 / 年龄 / 左手还是右手 / 师承……）：作者自己定键名。
    pub attributes: Vec<Attribute>,
    /// 作者自己写的一句备注。
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 新建 / 整卡更新一张设定卡要给的字段。
///
/// 更新走的是**整卡覆盖**（不是稀疏补丁）：这张卡就这么点字段，界面上一个表单全摆着，
/// 再搞一套"只改传进来的项"的合并，只会多一层容易写歪的语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntityCard {
    pub work_id: i64,
    pub kind: EntityKind,
    pub name: String,
    pub aliases: Vec<String>,
    pub attributes: Vec<Attribute>,
    pub note: String,
}

/// 设定卡的一行原样读出来（JSON 两列还是文本，认得认不得交给解析那一处）。
pub(crate) struct RawEntityCard {
    pub id: i64,
    pub work_id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub aliases: String,
    pub attributes: String,
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 把库里那一行变成一张卡。
///
/// 两个 JSON 列**坏了就报错**（不静默当空）：它们要拿去做冲突检测，
/// 悄悄少一截等于"少报一处冲突"——那比报错难查得多（与问题卡的"不猜着读"同一条纪律）。
pub(crate) fn into_entity_card(raw: RawEntityCard) -> Result<EntityCard> {
    let aliases: Vec<String> = serde_json::from_str(&raw.aliases).map_err(|_| bad(&raw, "aliases"))?;
    let attributes: Vec<Attribute> =
        serde_json::from_str(&raw.attributes).map_err(|_| bad(&raw, "attributes"))?;
    Ok(EntityCard {
        id: raw.id,
        // 与问题卡同一条口径：库里被手改成没有归属的按作品 0 算（如实显示，不假装有主）
        work_id: raw.work_id.unwrap_or(0),
        kind: EntityKind::parse(&raw.kind)?,
        name: raw.name,
        aliases,
        attributes,
        note: raw.note,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
    })
}

/// 「这一列存坏了」：把坏在哪一列说清楚（值可能很长，只带前 40 个字符）。
fn bad(raw: &RawEntityCard, field: &str) -> Error {
    let value = if field == "aliases" { &raw.aliases } else { &raw.attributes };
    let head: String = value.chars().take(40).collect();
    Error::invalid_with(
        codes::ENTITY_BAD_FIELD,
        [("field", field.to_string()), ("value", head)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_codes_round_trip_and_reject_strangers() {
        for kind in EntityKind::ALL {
            assert_eq!(EntityKind::parse(kind.as_str()).unwrap(), kind);
        }
        let err = EntityKind::parse("creature").unwrap_err();
        assert_eq!(err.code(), codes::UNKNOWN_ENTITY_KIND);
    }

    #[test]
    fn a_broken_json_column_is_reported_with_the_column_name() {
        let raw = RawEntityCard {
            id: 1,
            work_id: Some(1),
            kind: "person".to_string(),
            name: "陆文".to_string(),
            aliases: "不是 JSON".to_string(),
            attributes: "[]".to_string(),
            note: String::new(),
            created_at: 0,
            updated_at: 0,
        };
        let err = into_entity_card(raw).unwrap_err();
        assert_eq!(err.code(), codes::ENTITY_BAD_FIELD);
        assert!(err.params().contains(&("field", "aliases".to_string())));
    }
}
