//! 清单（manifest）的解析失败：**码 + 字段 + 技术细节**。
//!
//! 与核心的 `error_codes` 同一条纪律：码是稳定的机器标识，界面查自己的字典渲染；
//! 这里的 `detail` 只给日志与开发者看（英文技术消息，不冒充界面文案）。
//!
//! 为什么协议 crate 自带一套码、而不是复用核心的：`yanmo-proto` 是**公开协议层**，
//! 它不能反过来依赖 AGPL 核心；核心在安装模块时把这些码翻成自己的错误码即可。

use std::fmt;

/// 一次清单校验失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    /// 稳定码（见本类型的关联常量；**别改已发布的取值**）
    pub code: &'static str,
    /// 出问题的字段（`id` / `name` / `capabilities.read` …）
    pub field: &'static str,
    /// 技术细节（日志与开发者看；界面文案在宿主自己的字典里）
    pub detail: String,
}

impl ManifestError {
    /// JSON 本身坏了（不是合法 JSON / 多字段 / 少字段）。
    pub const JSON: &'static str = "manifest.json_invalid";
    /// 模块 id 不合规（不是反向域名式的小写 id）。
    pub const ID: &'static str = "manifest.id_invalid";
    /// 名字（按语言的字典）不合规：缺默认语言、语言码不认识、空串、超长。
    pub const NAME: &'static str = "manifest.name_invalid";
    /// 说明（按语言的字典）不合规。
    pub const DESCRIPTION: &'static str = "manifest.description_invalid";
    /// 作者信息不合规。
    pub const AUTHOR: &'static str = "manifest.author_invalid";
    /// 版本号不是 `x.y.z`。
    pub const VERSION: &'static str = "manifest.version_invalid";
    /// 目标协议版本高于本核心（要提示"请更新研墨"）。
    pub const PROTOCOL: &'static str = "manifest.protocol_unsupported";
    /// 入口路径不合规（绝对路径 / 跳出目录 / 不是 .wasm）。
    pub const ENTRY: &'static str = "manifest.entry_invalid";
    /// 能力声明不合规（未知值 / 重复 / 跨模块读自己不认识的 id）。
    pub const CAPABILITY: &'static str = "manifest.capability_invalid";
    /// 最低核心版本不是 `x.y.z`。
    pub const MIN_CORE: &'static str = "manifest.min_core_invalid";

    pub(crate) fn new(code: &'static str, field: &'static str, detail: impl Into<String>) -> Self {
        Self { code, field, detail: detail.into() }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.field, self.detail)
    }
}

impl std::error::Error for ManifestError {}

/// 把 serde 的报错收成一条可读的 detail（去掉换行，日志里一行看得完）。
pub(crate) fn serde_detail(error: &serde_json::Error) -> String {
    error.to_string().replace(['\n', '\r'], " ")
}
