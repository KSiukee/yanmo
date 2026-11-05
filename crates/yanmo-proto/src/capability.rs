//! 模块能要什么：**只读核心数据的范围 + 自己的数据 + 跨模块读 + 叩问 + 联网**。
//!
//! 五样都**默认关**（清单里不写就是没有）。能力清单是"模块声明 + 核心授予"的一半：
//! 这里只描述模块**声明**了什么；核心授不授予、用户同不同意，是注册表与界面的事。
//!
//! # 为什么只读范围是闭集
//!
//! 开放字符串会让"模块到底能读什么"变成一句空话（同一个数据两种叫法、拼错也照收）。
//! 闭集让权限提示能逐项本地化、让核心能机械核对"这个模块碰不到正文"。
//! 加一个可读区域＝在这里加一个取值，并同步更新界面字典。

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ManifestError;
use crate::id::is_valid_module_id;

/// 一条声明里最多列几个只读区域（当前闭集一共 8 个；重复也算超）。
pub const MAX_READ_AREAS: usize = 8;

/// 最多声明读几个别的模块（再多人肉审不过来）。
pub const MAX_CROSS_READ: usize = 16;

/// 核心数据里**可被模块只读**的区域（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoreDataArea {
    /// 作品元数据：标题 / 类型 / 语言 / 简介 / 故事总纲 / 分类
    Works,
    /// 目录树：卷 / 章 / 节 / 场景卡的结构与顺序
    Tree,
    /// 正文（整章级；**不提供逐字符接口**，见协议原则）
    Body,
    /// 大纲：每章一句话 / 章节目标 / 故事线标注
    Outline,
    /// 碎片池：灵感 / 事件 / 口述 / 答案
    Fragments,
    /// 设定卡：人物 / 设定（含属性）
    Entities,
    /// 场景卡的四格（视角 / 目标 / 冲突 / 结果）
    Scenes,
    /// 伏笔账本
    Foreshadows,
}

impl CoreDataArea {
    /// 全部取值（穷举测试与权限提示逐项渲染用）。
    pub const ALL: [CoreDataArea; 8] = [
        CoreDataArea::Works,
        CoreDataArea::Tree,
        CoreDataArea::Body,
        CoreDataArea::Outline,
        CoreDataArea::Fragments,
        CoreDataArea::Entities,
        CoreDataArea::Scenes,
        CoreDataArea::Foreshadows,
    ];

    /// 稳定码（写进清单 JSON 与核心的能力记录；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            CoreDataArea::Works => "works",
            CoreDataArea::Tree => "tree",
            CoreDataArea::Body => "body",
            CoreDataArea::Outline => "outline",
            CoreDataArea::Fragments => "fragments",
            CoreDataArea::Entities => "entities",
            CoreDataArea::Scenes => "scenes",
            CoreDataArea::Foreshadows => "foreshadows",
        }
    }
}

/// 手写序列化：只认 `as_str()` 这一份稳定码（不靠"改蛇形"来碰运气）。
impl Serialize for CoreDataArea {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CoreDataArea {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        CoreDataArea::ALL
            .into_iter()
            .find(|area| area.as_str() == value)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown core data area: {value}")))
    }
}

/// 模块声明的能力（**缺省全拒**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ModuleCapabilities {
    /// 只读核心数据的哪些区域
    pub read: Vec<CoreDataArea>,
    /// 自己的数据命名空间（`modules.db` 里 `mod_<id>_*`）可读写
    pub own_data: bool,
    /// 跨模块读谁（模块 id；对方也要声明被读的契约，见数据契约）
    pub cross_read: Vec<String>,
    /// 可否提交叩问（还要过通道的频率与凭据规则）
    pub ask_questions: bool,
    /// 可否联网（还要过全局离线开关、用户同意与出网代理）
    pub network: bool,
    /// 可否把正文内容**交给核心**（模块交付：默认关；数据层还要求带署名，缺署名拒收）
    ///
    /// 与 `read.body` 不是一回事：那个是"读出来"，这个是"写进去"。
    /// 口径：**默认关、没勾就拒收**（命令入口那一道闸）。
    pub deliver_body: bool,
}

impl ModuleCapabilities {
    /// 一个都不给（默认值就是它；写出来让调用方一眼看见"全拒"）。
    pub fn none() -> Self {
        Self::default()
    }

    /// 校验声明 / 授予本身：重复、跨模块读自己的 id、非法的模块 id、超过上限都拒。
    ///
    /// 注册表在**授予**时也用它：授予同样是"这个模块能碰什么"的一份声明，
    /// 不能因为来自界面就少校验一道。
    pub fn validate(&self, own_id: &str) -> Result<(), ManifestError> {
        if self.read.len() > MAX_READ_AREAS {
            return Err(ManifestError::new(
                ManifestError::CAPABILITY,
                "capabilities.read",
                format!("more than {MAX_READ_AREAS} read areas"),
            ));
        }
        let mut areas = BTreeSet::new();
        for area in &self.read {
            if !areas.insert(*area) {
                return Err(ManifestError::new(
                    ManifestError::CAPABILITY,
                    "capabilities.read",
                    format!("duplicate read area: {}", area.as_str()),
                ));
            }
        }

        if self.cross_read.len() > MAX_CROSS_READ {
            return Err(ManifestError::new(
                ManifestError::CAPABILITY,
                "capabilities.cross_read",
                format!("more than {MAX_CROSS_READ} cross-read modules"),
            ));
        }
        let mut modules = BTreeSet::new();
        for module_id in &self.cross_read {
            if !is_valid_module_id(module_id) {
                return Err(ManifestError::new(
                    ManifestError::CAPABILITY,
                    "capabilities.cross_read",
                    format!("invalid module id: {module_id}"),
                ));
            }
            if module_id == own_id {
                return Err(ManifestError::new(
                    ManifestError::CAPABILITY,
                    "capabilities.cross_read",
                    "a module cannot declare reading itself",
                ));
            }
            if !modules.insert(module_id.as_str()) {
                return Err(ManifestError::new(
                    ManifestError::CAPABILITY,
                    "capabilities.cross_read",
                    format!("duplicate module id: {module_id}"),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_codes_round_trip_in_json() {
        for area in CoreDataArea::ALL {
            let json = serde_json::to_string(&area).unwrap();
            assert_eq!(json, format!("\"{}\"", area.as_str()));
            let back: CoreDataArea = serde_json::from_str(&json).unwrap();
            assert_eq!(back, area);
        }
        assert!(serde_json::from_str::<CoreDataArea>("\"everything\"").is_err());
    }

    #[test]
    fn missing_capabilities_mean_nothing() {
        let caps: ModuleCapabilities = serde_json::from_str("{}").unwrap();
        assert_eq!(caps, ModuleCapabilities::none());
        assert!(!caps.own_data && !caps.ask_questions && !caps.network);
        assert!(!caps.deliver_body, "交付正文默认关");
    }

    #[test]
    fn duplicate_and_self_cross_read_are_rejected() {
        let mut caps = ModuleCapabilities::none();
        caps.cross_read = vec!["app.example.other".to_string(), "app.example.other".to_string()];
        assert_eq!(
            caps.validate("app.example.me").unwrap_err().code,
            ManifestError::CAPABILITY
        );
        caps.cross_read = vec!["app.example.me".to_string()];
        assert_eq!(
            caps.validate("app.example.me").unwrap_err().code,
            ManifestError::CAPABILITY
        );
        caps.cross_read = vec!["not a module".to_string()];
        assert_eq!(
            caps.validate("app.example.me").unwrap_err().code,
            ManifestError::CAPABILITY
        );
    }

    #[test]
    fn duplicate_read_areas_are_rejected() {
        let caps = ModuleCapabilities {
            read: vec![CoreDataArea::Body, CoreDataArea::Body],
            ..ModuleCapabilities::none()
        };
        assert_eq!(
            caps.validate("app.example.me").unwrap_err().code,
            ManifestError::CAPABILITY
        );
    }
}
