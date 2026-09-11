//! 核心统一错误类型。
//!
//! 保持极小：只区分"调用方需要分别处理"的类别，别长成错误枚举垃圾桶。

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// 数据库层错误（含 SQLite 自身）
    Db(rusqlite::Error),
    /// 数据格式版本高于本引擎能理解的上限（拒绝打开，而不是猜着读）
    SchemaTooNew { found: u32, supported: u32 },
    /// 运行环境不满足要求（如 SQLite 缺 FTS5）
    Unsupported(String),
    /// 违反数据约束（调用方传了非法值）
    Invalid(String),
    /// 文件系统错误（原子写 / 导出逃生 / 磁盘满等）
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Db(e) => write!(f, "数据库错误：{e}"),
            Error::SchemaTooNew { found, supported } => write!(
                f,
                "数据格式版本 {found} 高于本引擎支持的 {supported}——请升级研墨，不要用旧版打开新库"
            ),
            Error::Unsupported(msg) => write!(f, "运行环境不支持：{msg}"),
            Error::Invalid(msg) => write!(f, "数据不合法：{msg}"),
            Error::Io(e) => write!(f, "文件读写失败：{e}"),
        }
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
