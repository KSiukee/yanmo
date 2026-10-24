//! 模块生命周期：**激活 / 冻结、启动来源、启动许可**。
//!
//! # 三条铁律
//!
//! 1. **任何模块都不许自行启动**——没有外部触发就常驻，等于偷机主的性能与电。
//!    [`ModuleLaunchSource`] 是一个**闭集**：用户打开界面、用户主动触发的动作、
//!    被别的模块唤起（成链）。**没有**"后台/开机/定时"这一类；未来宿主想启动模块，
//!    只能从这三条里选一条，没有第二条路。
//! 2. **未激活 = 冻结**：不可被调用、不可被唤醒、不占进程；只保留历史数据与图标。
//!    激活是**用户的开关**，与认证无关：已激活未认证可以跑，未激活一律冻结。
//! 3. **停止运行 ≠ 禁止后台运行**：前者是立即动作（宿主的按钮，先优雅退出、超时强杀），
//!    后者是策略（拒绝下次自启）。本文件只管**许可**；进程的停与杀在宿主那一层。
//!
//! # 与认证的关系
//!
//! **认证是官方背书，激活是用户的开关**。两件事别混：认证不给你启动权，
//! 激活也不代表官方审过。冻结时连"本人已认证"的模块也一律不可调。
//!
//! # 现在能落到哪一步
//!
//! 宿主运行时还没做（沙箱载荷 / 独立进程在后面的任务里）。这一层先把**许可**钉死：
//! 每次启动前必须拿一张 [`ModuleLaunchPermit`]（写进留痕），宿主拿着它才允许拉起。
//! 链式启动要等"链路可见 + 可断 + 频率上限"那一族落地；在那之前**明确拒绝并留痕**，
//! 不假装能管住一条看不见的链。

use serde::{Deserialize, Serialize};

use crate::error::{codes, Error, Result};

/// 模块的激活状态。
///
/// **没有"已卸载"这一档**：卸载 = 注册表里没有这条记录，不是一种状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleState {
    /// 已激活（装了默认就是它；用户可以停用）
    Active,
    /// 冻结（不可调用、不可唤醒、不占进程；数据与图标保留）
    Frozen,
}

impl ModuleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            ModuleState::Active => "active",
            ModuleState::Frozen => "frozen",
        }
    }
}

/// 一次启动的来源（**闭集**；没有"后台自启"这一条）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModuleLaunchSource {
    /// 用户主动打开它的界面
    UserOpen,
    /// 用户主动触发的动作（如导出时用到模板模块）；`action` 是动作的稳定名
    UserAction {
        /// 动作名（1~64 字，不能是空白 / 控制字符）
        action: String,
    },
    /// 被另一个模块唤起（成链）
    ModuleChain {
        /// 发起唤起的模块 id
        from: String,
    },
}

impl ModuleLaunchSource {
    /// 允许的来源码（**穷举**；测试与宿主对表用）。
    pub const ALLOWED: [&'static str; 3] = ["user_open", "user_action", "module_chain"];

    /// 稳定码（写进留痕）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            ModuleLaunchSource::UserOpen => "user_open",
            ModuleLaunchSource::UserAction { .. } => "user_action",
            ModuleLaunchSource::ModuleChain { .. } => "module_chain",
        }
    }

    /// 是不是用户主动发起的（链不是）。
    pub fn is_user_initiated(&self) -> bool {
        matches!(self, ModuleLaunchSource::UserOpen | ModuleLaunchSource::UserAction { .. })
    }

    /// 结构校验：动作名不能空 / 超长 / 带控制字符；链的发起方必须是合规模块 id。
    pub fn validate(&self) -> Result<()> {
        match self {
            ModuleLaunchSource::UserOpen => Ok(()),
            ModuleLaunchSource::UserAction { action } => {
                let action = action.trim();
                if action.is_empty() || action.chars().count() > 64 {
                    return Err(source_invalid("action must be 1..=64 characters"));
                }
                if action.chars().any(char::is_control) {
                    return Err(source_invalid("action must not contain control characters"));
                }
                Ok(())
            }
            ModuleLaunchSource::ModuleChain { from } => {
                if yanmo_proto::is_valid_module_id(from) {
                    Ok(())
                } else {
                    Err(source_invalid("from must be a valid module id"))
                }
            }
        }
    }
}

/// 一张启动许可：**宿主拿到它才允许拉起模块**。
///
/// 它不是可重放的令牌：每次启动前都要向核心要一张；要不到就不许启动。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleLaunchPermit {
    pub module_id: String,
    pub source: ModuleLaunchSource,
    pub granted_at: i64,
}

fn source_invalid(detail: impl Into<String>) -> Error {
    Error::invalid_with(codes::MODULE_LAUNCH_SOURCE_INVALID, [("detail", detail.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowed_sources_have_no_background_variant() {
        assert_eq!(
            ModuleLaunchSource::ALLOWED,
            ["user_open", "user_action", "module_chain"]
        );
        for source in ModuleLaunchSource::ALLOWED {
            assert!(!source.contains("background"));
            assert!(!source.contains("startup"));
            assert!(!source.contains("timer"));
        }
    }

    #[test]
    fn user_initiated_sources_are_recognised() {
        assert!(ModuleLaunchSource::UserOpen.is_user_initiated());
        assert!(ModuleLaunchSource::UserAction { action: "export".into() }.is_user_initiated());
        assert!(!ModuleLaunchSource::ModuleChain { from: "app.example.a".into() }.is_user_initiated());
    }

    #[test]
    fn launch_sources_are_validated() {
        assert!(ModuleLaunchSource::UserOpen.validate().is_ok());
        assert!(ModuleLaunchSource::UserAction { action: "export".into() }.validate().is_ok());
        for action in ["", "   ", &"x".repeat(65), "a\nb"] {
            assert!(
                ModuleLaunchSource::UserAction { action: action.to_string() }
                    .validate()
                    .is_err(),
                "action: {action:?}"
            );
        }
        assert!(ModuleLaunchSource::ModuleChain { from: "app.example.a".into() }.validate().is_ok());
        assert!(ModuleLaunchSource::ModuleChain { from: "not a module".into() }.validate().is_err());
    }
}
