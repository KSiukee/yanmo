//! 模块注册表里的一条：**它声明了什么、用户给了什么、现在能不能读**。
//!
//! 这里不解析清单（清单的解析与校验在公开协议层 `yanmo_proto`）；注册表只负责
//! 把"声明"与"授予"放在一起，并回答**读数据的门卫问题**：这个模块现在能不能读这一块。
//!
//! 三个布尔口径：
//! - `enabled`：没冻结（冻结即不可读、不可问、不可联网；数据保留）；
//! - `certified`：核心**验签之后**写的认证状态，不是模块自述；
//! - `granted`：用户实际给的能力（永远是声明的一个子集，注册表写入时校验）。

use serde::Serialize;
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

use super::module_lifecycle::ModuleState;

/// 注册表里的一条模块记录。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleRecord {
    /// 模块清单（模块自述的原文，已校验）
    pub manifest: ModuleManifest,
    /// 用户授予的能力（声明的一个子集；缺省全拒）
    pub granted: ModuleCapabilities,
    /// 没冻结
    pub enabled: bool,
    /// 核心验签后的认证状态（模块不能在清单里自述）
    pub certified: bool,
    pub installed_at: i64,
    pub updated_at: i64,
}

impl ModuleRecord {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// 激活状态：装了默认就是激活；用户停用即冻结。
    pub fn state(&self) -> ModuleState {
        if self.enabled {
            ModuleState::Active
        } else {
            ModuleState::Frozen
        }
    }

    /// 现在能不能被调用 / 唤醒（冻结即不能）。
    pub fn is_active(&self) -> bool {
        self.enabled
    }

    /// 现在能不能读核心的这块数据：**没冻结 + 授予了这一块**。
    pub fn can_read(&self, area: CoreDataArea) -> bool {
        self.enabled && self.granted.read.contains(&area)
    }

    /// 现在能不能读写自己的数据命名空间。
    pub fn may_own_data(&self) -> bool {
        self.enabled && self.granted.own_data
    }

    /// 现在能不能提交叩问（通道本身还要过频率与凭据）。
    pub fn may_ask(&self) -> bool {
        self.enabled && self.granted.ask_questions
    }

    /// 现在能不能联网（还要过全局离线开关与出网代理）。
    pub fn may_network(&self) -> bool {
        self.enabled && self.granted.network
    }

    /// 现在能不能读另一个模块的公开数据。
    pub fn may_cross_read(&self, other_module_id: &str) -> bool {
        self.enabled && self.granted.cross_read.iter().any(|id| id == other_module_id)
    }
}

/// 授予里第一项**没被声明**的能力（`None` = 是声明的子集）。
///
/// 返回的是给人看的能力名（`read.body` / `own_data` / `cross_read.app.x` /
/// `ask_questions` / `network`）：错误码只需要一个 `capability` 参数。
pub fn first_ungrantable(
    declared: &ModuleCapabilities,
    granted: &ModuleCapabilities,
) -> Option<String> {
    for area in &granted.read {
        if !declared.read.contains(area) {
            return Some(format!("read.{}", area.as_str()));
        }
    }
    if granted.own_data && !declared.own_data {
        return Some("own_data".to_string());
    }
    for module_id in &granted.cross_read {
        if !declared.cross_read.contains(module_id) {
            return Some(format!("cross_read.{module_id}"));
        }
    }
    if granted.ask_questions && !declared.ask_questions {
        return Some("ask_questions".to_string());
    }
    if granted.network && !declared.network {
        return Some("network".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(read: &[CoreDataArea], own_data: bool) -> ModuleCapabilities {
        ModuleCapabilities {
            read: read.to_vec(),
            own_data,
            cross_read: Vec::new(),
            ask_questions: false,
            network: false,
        }
    }

    #[test]
    fn a_grant_cannot_exceed_the_declaration() {
        let declared = caps(&[CoreDataArea::Works], false);
        assert!(first_ungrantable(&declared, &caps(&[CoreDataArea::Works], false)).is_none());
        assert_eq!(
            first_ungrantable(&declared, &caps(&[CoreDataArea::Body], false)).as_deref(),
            Some("read.body")
        );
        assert_eq!(
            first_ungrantable(&declared, &caps(&[CoreDataArea::Works], true)).as_deref(),
            Some("own_data")
        );
    }

    #[test]
    fn cross_read_and_questioning_are_also_subsets() {
        let mut declared = ModuleCapabilities::none();
        declared.cross_read = vec!["app.example.other".to_string()];
        let mut granted = ModuleCapabilities::none();
        granted.cross_read = vec!["app.example.other".to_string()];
        assert!(first_ungrantable(&declared, &granted).is_none());
        granted.cross_read = vec!["app.example.third".to_string()];
        assert_eq!(
            first_ungrantable(&declared, &granted).as_deref(),
            Some("cross_read.app.example.third")
        );

        let declared = ModuleCapabilities::none();
        let mut granted = ModuleCapabilities::none();
        granted.ask_questions = true;
        assert_eq!(first_ungrantable(&declared, &granted).as_deref(), Some("ask_questions"));
        granted.ask_questions = false;
        granted.network = true;
        assert_eq!(first_ungrantable(&declared, &granted).as_deref(), Some("network"));
    }
}
