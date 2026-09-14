//! 核心统一错误类型：**只给「码 + 参数」，不给人类语言**。
//!
//! 保持极小：只区分"调用方需要分别处理"的类别，别长成错误枚举垃圾桶。
//!
//! # 为什么不是「错误消息字符串」
//!
//! 核心零 UI 依赖（见 `lib.rs` 的铁律）不只是"不 import 界面框架"，
//! 也包括**不生产界面文案**：一旦核心输出中文句子，界面语言就被焊死在核心里，
//! 换语言要改核心、模块报错也要核心替它说话。
//!
//! 所以：
//! - [`Error::code`] 给**码**（点分小写英文，见 `codes`），[`Error::params`] 给**参数**；
//! - 界面拿码查自己的字典渲染成句子；
//! - `Display` 仍然有中文——那是**给日志与开发者**看的（模板在 [`crate::error_codes`]），
//!   与界面文案是两件事，谁也别冒充谁。

use std::fmt;

pub use crate::error_codes::codes;
pub use crate::error_codes::ERROR_CODES;

/// 核心统一的结果类型（失败一律是 [`Error`]）。
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
/// 核心的错误类型：**带码与参数**，句子由界面查字典渲染。
pub enum Error {
    /// 数据库层错误（含 SQLite 自身）
    Db(rusqlite::Error),
    /// 数据格式版本高于本引擎能理解的上限（拒绝打开，而不是猜着读）
    SchemaTooNew { found: u32, supported: u32 },
    /// 运行环境不满足要求（如 SQLite 缺 FTS5）
    Unsupported { code: &'static str, params: Vec<(&'static str, String)> },
    /// 违反数据约束（调用方传了非法值）
    Invalid { code: &'static str, params: Vec<(&'static str, String)> },
    /// 文件系统错误（原子写 / 导出逃生 / 磁盘满等）
    Io(std::io::Error),
}

impl Error {
    /// 没有附加参数的「数据不合法」。
    pub fn invalid(code: &'static str) -> Self {
        Self::invalid_with(code, [])
    }

    /// 带参数的「数据不合法」：参数只放**取值**，别塞成品句子。
    pub fn invalid_with(
        code: &'static str,
        params: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        let params = collect(code, params);
        Error::Invalid { code, params }
    }

    /// 没有附加参数的「运行环境不支持」。
    pub fn unsupported(code: &'static str) -> Self {
        Self::unsupported_with(code, [])
    }

    /// 带参数的「运行环境不支持」。
    pub fn unsupported_with(
        code: &'static str,
        params: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        let params = collect(code, params);
        Error::Unsupported { code, params }
    }

    /// 面向界面的**错误码**——界面查字典就靠它。
    pub const fn code(&self) -> &'static str {
        match self {
            Error::Db(_) => codes::DB,
            Error::SchemaTooNew { .. } => codes::SCHEMA_TOO_NEW,
            Error::Unsupported { code, .. } | Error::Invalid { code, .. } => code,
            Error::Io(_) => codes::IO,
        }
    }

    /// 面向界面的**参数**：`(名字, 取值)`，名字与日志模板里的 `{名字}` 对应。
    ///
    /// 数据库 / 文件系统的底层消息作为 `detail` 参数传出——那是技术细节，不是界面句子，
    /// 界面把它当"出错的证据"原样附在后面，比自己编一句"操作失败"有用得多。
    pub fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            Error::Db(e) => vec![("detail", e.to_string())],
            Error::SchemaTooNew { found, supported } => vec![
                ("found", found.to_string()),
                ("supported", supported.to_string()),
            ],
            Error::Unsupported { params, .. } | Error::Invalid { params, .. } => params.clone(),
            Error::Io(e) => vec![("detail", e.to_string())],
        }
    }
}

/// 登记前的把门动作：码必须出自 `codes`，参数名必须与码表声明的**正好一致**，
/// 且日志模板里的占位符都有对应参数。
///
/// 放在构造函数里而不是测试里，是因为**错在这里就要当场炸**——
/// 一个没登记的码会一路飘到界面变成"未知错误"，参数名漂了则会让界面显示成 `{node_id}`，
/// 两种都难查。
fn collect(
    code: &'static str,
    params: impl IntoIterator<Item = (&'static str, String)>,
) -> Vec<(&'static str, String)> {
    let params: Vec<(&'static str, String)> = params.into_iter().collect();
    let declared = crate::error_codes::code_params(code);
    // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
    debug_assert!(declared.is_some(), "错误码 {code} 没登记进 error_codes.rs 的码表");
    if let Some(declared) = declared {
        let mut provided: Vec<&str> = params.iter().map(|(name, _)| *name).collect();
        provided.sort_unstable();
        provided.dedup();
        let mut expected: Vec<&str> = declared.to_vec();
        expected.sort_unstable();
        // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
        debug_assert_eq!(
            provided, expected,
            "码 {code} 的参数名与码表声明对不上：调用点给了 {provided:?}，码表写的是 {expected:?}"
        );
        // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
        debug_assert!(
            crate::error_codes::render_log(code, &params).find('{').is_none(),
            "码 {code} 的日志模板里有没被填上的占位符——参数名单对不上"
        );
    }
    params
}

impl fmt::Display for Error {
    /// 给日志与开发者：中文句子。**界面不许拿它当文案**（界面走 `code()` + `params()`）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", crate::error_codes::render_log(self.code(), &self.params()))
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_carry_their_params() {
        let e = Error::invalid_with(codes::NODE_GONE, [("node_id", "42".to_string())]);
        assert_eq!(e.code(), "node.gone");
        assert_eq!(e.params(), vec![("node_id", "42".to_string())]);
    }

    #[test]
    fn display_speaks_chinese_for_the_log() {
        let e = Error::invalid(codes::WORK_TITLE_EMPTY);
        assert_eq!(e.to_string(), "作品标题不能为空");

        let e = Error::invalid_with(codes::WORK_NOT_TRASHED, [("work_id", "7".to_string())]);
        assert_eq!(e.to_string(), "回收站里没有这本书：7");
    }
}
