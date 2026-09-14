//! 三源版本体系。
//!
//! 三个版本**分开**，因为它们变化的原因不同，多端同步时要分别比对：
//!
//! | 版本 | 什么时候变 | 谁派生 |
//! |---|---|---|
//! | 引擎版本 | 每次发布 | 编译期（`CARGO_PKG_VERSION`） |
//! | **schema 版本** | 表结构变更时 | **派生自迁移表末位**，不手写（防漂移） |
//! | 数据格式版本 | 磁盘 `.md` 镜像 / 导出格式变更时 | 手写常量 |
//!
//! 另有一个 `channel`（stable / dev），更新器按渠道选源。

pub use crate::db::migrations::schema_version;

/// 发布渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Dev,
}

impl Channel {
    /// 稳定代码（写进日志与设置的「关于」）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Dev => "dev",
        }
    }
}

/// 引擎版本（等于 crate 版本，由 Cargo 在编译期注入）。
pub const fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 引擎版本字符串（运行时取用，同 [`engine_version`]）。
pub fn version_string() -> String {
    engine_version().to_string()
}

/// **数据格式版本**：磁盘 `.md` 镜像与导出文件的格式版本。
///
/// 与 schema 版本分开：库结构可以升级而磁盘格式不变，反之亦然。
pub const DATA_FORMAT_VERSION: u32 = 1;

/// 发布渠道（当前为开发期）。
pub const CHANNEL: Channel = Channel::Dev;

/// 三版本汇总——更新器与多端同步的比对入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub engine: &'static str,
    pub schema: u32,
    pub data_format: u32,
    pub channel: &'static str,
}

impl VersionInfo {
    /// 单行可解析表示（缺项用 `?` 占位也保持可解析）。
    pub fn to_line(&self) -> String {
        format!(
            "engine={} schema={} data={} channel={}",
            self.engine, self.schema, self.data_format, self.channel
        )
    }
}

/// 汇总三版本。
pub fn describe() -> VersionInfo {
    VersionInfo {
        engine: engine_version(),
        schema: schema_version(),
        data_format: DATA_FORMAT_VERSION,
        channel: CHANNEL.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!engine_version().is_empty());
    }

    #[test]
    fn schema_version_is_derived_not_hardcoded() {
        // schema 版本必须等于迁移表末位（防手写漂移）
        assert_eq!(schema_version(), crate::db::migrations::MIGRATIONS.last().unwrap().version);
    }

    #[test]
    fn describe_is_parseable() {
        let line = describe().to_line();
        for key in ["engine=", "schema=", "data=", "channel="] {
            assert!(line.contains(key), "缺少 {key}：{line}");
        }
    }
}
