//! 模块 id：**反向域名式、全小写、至少两段**。
//!
//! 为什么要这么窄：id 会进目录名（`modules/<id>/`）、`modules.db` 的表名前缀
//! （`mod_<id>_*`）、问题卡的 `source` 列与界面标签。放开了写，路径穿越、大小写撞车、
//! 表名前缀污染都会从这一格进来。
//!
//! 规则（与清单校验共用这一处）：
//! - 总长 3~128 字符，纯 ASCII；
//! - 至少两段，段间用 `.` 分隔；每段 1~63 字符；
//! - 段内只允许 `a-z` / `0-9` / `-`，且首尾不能是 `-`；
//! - 不许空段（`a..b`）、不许首尾点（`.a.b` / `a.b.`）。
//!
//! `app.yanmo.*` 按约定留给官方模块（第三方用自己的域名）——这是**命名约定**，
//! 协议层只管格式、不替谁裁决归属；"不得冒充官方"由界面与政策那一层管。

/// 模块 id 的最大长度（字符）。
pub const MAX_MODULE_ID_CHARS: usize = 128;

/// 单段的最大长度（DNS 习惯）。
pub const MAX_SEGMENT_CHARS: usize = 63;

/// 合不合规（见模块文档的三条规则）。
pub fn is_valid_module_id(id: &str) -> bool {
    if id.len() < 3 || id.len() > MAX_MODULE_ID_CHARS || !id.is_ascii() {
        return false;
    }
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty()
            && segment.len() <= MAX_SEGMENT_CHARS
            && !segment.starts_with('-')
            && !segment.ends_with('-')
            && segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_reverse_domain_ids() {
        for id in ["app.yanmo.map", "com.example.plot-map", "io.github.someone.tool2"] {
            assert!(is_valid_module_id(id), "应接受：{id}");
        }
    }

    #[test]
    fn rejects_paths_case_and_empty_segments() {
        let mut bad: Vec<String> = [
            "",
            "map",
            "Map.example",
            "app..map",
            ".app.map",
            "app.map.",
            "-app.map",
            "app.-map",
            "app/ma p",
            "app.map/../evil",
            "app.地图",
        ]
        .iter()
        .map(|id| (*id).to_string())
        .collect();
        bad.push(format!("app.{}", "x".repeat(MAX_SEGMENT_CHARS + 1)));
        bad.push(format!("a.{}", "x".repeat(MAX_MODULE_ID_CHARS)));
        for id in bad {
            assert!(!is_valid_module_id(&id), "应拒绝：{id:?}");
        }
    }
}
