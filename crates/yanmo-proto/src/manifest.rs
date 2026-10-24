//! 模块清单（`modules/<id>/manifest.json`）：**协议里唯一由模块自述的东西**。
//!
//! 它只回答五件事：我是谁、我叫什么、我怎么跑、我要什么能力、我的界面长什么样。
//!
//! # 什么**不**在清单里（故意的）
//!
//! - **认证状态**：模块自述"我已认证"一文不值。认证是核心**验签之后**写进注册表的
//!   事实（见认证与防伪那一族），清单里没有这一格；
//! - **权限提示文案**：提示必须由宿主渲染并本地化（模块不能自述权限），
//!   所以清单只给**能力码**，句子在核心的字典里；
//! - **付费/授权状态**：核心是宿主、不是授权方；它不知道谁付了钱。
//!
//! # 装载形态
//!
//! 当前只有一种：[`ModuleRuntime::Wasm`]（沙箱载荷）。原生 exe 档**不开**——
//! 模块在插件系统之外不可自运行，这是能力清单能真正拦住东西的前提。
//! 语言档先 Rust（编成 WASM），JS 作为明确标注的慢档后补。
//!
//! # 界面形态三档
//!
//! 内嵌（分栏 / 抽屉 / 弹窗 / 全屏工作台）、独立窗口、无界面。内嵌用的子 WebView
//! **既不能拿宿主 IPC、也不能自己出网**（CSP 收死 + 只允许自定义协议 + 外发一律走宿主代理）——
//! 这是实现时必须验证的硬约束，不是清单能保证的事。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::ModuleCapabilities;
use crate::error::{serde_detail, ManifestError};
use crate::id::{is_valid_module_id, MAX_MODULE_ID_CHARS};
use crate::localized::validate_localized;
use crate::version::parse_semver;
use crate::PROTOCOL_VERSION;

/// 名字（每种语言）的最大长度。
pub const MAX_NAME_CHARS: usize = 120;
/// 一句话说明的最大长度。
pub const MAX_DESCRIPTION_CHARS: usize = 600;
/// 作者名的最大长度。
pub const MAX_AUTHOR_CHARS: usize = 120;
/// 作者主页的最大长度。
pub const MAX_HOMEPAGE_CHARS: usize = 300;
/// 入口路径的最大长度。
pub const MAX_ENTRY_CHARS: usize = 200;

/// 载荷形态（当前只有沙箱 WASM）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRuntime {
    /// 沙箱载荷：`entry` 指向模块目录内的 `.wasm`
    Wasm,
}

/// 内嵌界面的落位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiPlacement {
    /// 分栏（三栏布局里多一栏）
    Pane,
    /// 抽屉（从侧边滑出）
    Drawer,
    /// 弹窗
    Dialog,
    /// 全屏工作台
    Workbench,
}

/// 界面形态三档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "snake_case")]
pub enum ModuleUiForm {
    /// 内嵌：模块自己起本地服务，宿主开一块子 WebView 装进来
    Embedded { placement: UiPlacement },
    /// 独立窗口：模块自己开原生窗口（兼容档，留给本来就是大工具的模块）
    Standalone,
    /// 无界面：只做后台处理
    Headless,
}

/// 作者 / 来源（第三方模块**必须**能看出是谁做的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleAuthor {
    /// 作者或组织名（显示用；不是 id）
    pub name: String,
    /// 主页 / 仓库地址（可选；只接受 http(s)）
    #[serde(default)]
    pub homepage: Option<String>,
}

/// 模块清单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleManifest {
    /// 反向域名式 id（`app.example.map`）
    pub id: String,
    /// 名字与说明共用的缺省语言（必须出现在两张字典里）
    pub default_language: String,
    /// 名字：语言码 → 文本
    pub name: BTreeMap<String, String>,
    /// 一句话说明：语言码 → 文本
    pub description: BTreeMap<String, String>,
    pub author: ModuleAuthor,
    /// 模块自身版本 `x.y.z`
    pub version: String,
    /// 目标协议版本（不能高于本核心）
    pub protocol: u32,
    /// 最低核心版本 `x.y.z`（可选；不够时提示"请更新研墨"）
    #[serde(default)]
    pub min_core_version: Option<String>,
    pub runtime: ModuleRuntime,
    /// 模块目录内的相对入口（WASM 载荷）
    pub entry: String,
    pub ui: ModuleUiForm,
    /// 能力声明（整格必须写；格内缺省全拒）
    pub capabilities: ModuleCapabilities,
}

impl ModuleManifest {
    /// 从 JSON 文本解析并校验。**严格**：多一个字段都拒（不认识的清单不是自己人）。
    pub fn from_json(text: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(text).map_err(|error| {
            ManifestError::new(ManifestError::JSON, "manifest", serde_detail(&error))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// 只做字段校验（JSON 已经解析好时用）。
    pub fn validate(&self) -> Result<(), ManifestError> {
        if !is_valid_module_id(&self.id) {
            return Err(ManifestError::new(
                ManifestError::ID,
                "id",
                format!("not a valid reverse-domain module id (max {MAX_MODULE_ID_CHARS} chars)"),
            ));
        }
        validate_localized(
            &self.name,
            &self.default_language,
            "name",
            ManifestError::NAME,
            MAX_NAME_CHARS,
        )?;
        validate_localized(
            &self.description,
            &self.default_language,
            "description",
            ManifestError::DESCRIPTION,
            MAX_DESCRIPTION_CHARS,
        )?;

        if self.author.name.trim().is_empty() || self.author.name.chars().count() > MAX_AUTHOR_CHARS {
            return Err(ManifestError::new(
                ManifestError::AUTHOR,
                "author.name",
                "author name must be 1..=120 characters",
            ));
        }
        if let Some(homepage) = &self.author.homepage {
            if !valid_homepage(homepage) {
                return Err(ManifestError::new(
                    ManifestError::AUTHOR,
                    "author.homepage",
                    "homepage must be an http(s) URL",
                ));
            }
        }

        if parse_semver(&self.version).is_none() {
            return Err(ManifestError::new(
                ManifestError::VERSION,
                "version",
                "version must be x.y.z",
            ));
        }
        if self.protocol == 0 || self.protocol > PROTOCOL_VERSION {
            return Err(ManifestError::new(
                ManifestError::PROTOCOL,
                "protocol",
                format!(
                    "module targets protocol {}, this core supports {}",
                    self.protocol, PROTOCOL_VERSION
                ),
            ));
        }
        if let Some(min) = &self.min_core_version {
            if parse_semver(min).is_none() {
                return Err(ManifestError::new(
                    ManifestError::MIN_CORE,
                    "min_core_version",
                    "min_core_version must be x.y.z",
                ));
            }
        }

        if !valid_entry(&self.entry, self.runtime) {
            return Err(ManifestError::new(
                ManifestError::ENTRY,
                "entry",
                "entry must be a relative .wasm path inside the module directory",
            ));
        }
        self.capabilities.validate(&self.id)?;
        Ok(())
    }

    /// 本核心能不能跑这个清单（协议版本够不够）。
    pub fn compatible_with_core(&self) -> bool {
        self.protocol >= 1 && self.protocol <= PROTOCOL_VERSION
    }

    /// 核心版本够不够清单写的最低要求（没写就是 `0.0.0`，永远够）。
    pub fn accepts_core_version(&self, core: crate::version::Version) -> bool {
        match parse_semver(self.min_core_version.as_deref().unwrap_or("0.0.0")) {
            Some(minimum) => core >= minimum,
            None => false,
        }
    }

    /// 取名字（按界面语言回退：精确 → 同主语言 → 默认语言 → 第一条）。
    pub fn name_text(&self, locale: &str) -> &str {
        crate::localized::localized_text(&self.name, &self.default_language, locale)
    }

    /// 取一句话说明（同一套回退规则）。
    pub fn description_text(&self, locale: &str) -> &str {
        crate::localized::localized_text(&self.description, &self.default_language, locale)
    }
}

fn valid_homepage(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= MAX_HOMEPAGE_CHARS
        && !value.chars().any(char::is_whitespace)
        && (value.starts_with("https://") || value.starts_with("http://"))
}

/// 入口只能是模块目录内的相对 `.wasm` 路径（不许绝对路径、不许跳出目录）。
fn valid_entry(entry: &str, runtime: ModuleRuntime) -> bool {
    if entry.is_empty() || entry.chars().count() > MAX_ENTRY_CHARS {
        return false;
    }
    if entry.starts_with('/') || entry.starts_with('\\') || entry.contains('\\') || entry.contains(':') {
        return false;
    }
    if entry.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return false;
    }
    match runtime {
        ModuleRuntime::Wasm => entry.ends_with(".wasm"),
    }
}
