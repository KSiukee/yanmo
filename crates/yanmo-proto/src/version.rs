//! 版本号：协议里只认**三段数字**（`x.y.z`）。
//!
//! 为什么不用完整 SemVer（预发布 / 构建元数据）：模块清单是**安装时的门槛**，
//! 它只回答"这个模块/核心够不够新"；预发布那套排序规则用不上，却会带来两套实现
//! 必须逐字一致的负担。等真有预发布需求再扩展——扩展时只加后缀解析，不改前三段。

use std::fmt;

/// 一个三段版本号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// 解析 `x.y.z`；不合法返回 `None`（不猜、不补零）。
///
/// 规则：三段、每段纯数字、不许前导零（`01` 不是合法写法）、每段 ≤ `u32::MAX`。
pub fn parse_semver(text: &str) -> Option<Version> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut numbers = [0u32; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty()
            || !part.chars().all(|c| c.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return None;
        }
        numbers[index] = part.parse().ok()?;
    }
    Some(Version { major: numbers[0], minor: numbers[1], patch: numbers[2] })
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_numeric_parts_only() {
        assert_eq!(parse_semver("1.2.3"), Some(Version { major: 1, minor: 2, patch: 3 }));
        assert_eq!(parse_semver("0.0.0"), Some(Version { major: 0, minor: 0, patch: 0 }));
        for bad in ["1.2", "1.2.3.4", "1.2.x", "01.2.3", "1.02.3", "1.2.03", "", "1..3", " 1.2.3"] {
            assert_eq!(parse_semver(bad), None, "应拒绝：{bad:?}");
        }
    }

    #[test]
    fn orders_by_major_then_minor_then_patch() {
        let parse = |text| parse_semver(text).unwrap();
        assert!(parse("2.0.0") > parse("1.99.99"));
        assert!(parse("1.3.0") > parse("1.2.99"));
        assert!(parse("1.2.4") > parse("1.2.3"));
    }
}
