//! 命名空间归属：`modules.db` 里的每张表**到底是谁的**。
//!
//! # 一条不用猜的切法
//!
//! 模块 id 里**没有下划线**（见 [`yanmo_proto::is_valid_module_id`]：只允许
//! `a-z` / `0-9` / `-` / `.`），所以 `mod_` 之后的**第一个**下划线就是
//! 「id 与表后缀」的分界——不用按模块清单挨个试前缀，也不会把两张表认成一家。
//!
//! # 四种归属
//!
//! - [`TableOwner::Core`]：核心自己的记账表（`module_schema` / `module_data_health` /
//!   `module_data_isolation`）与 SQLite 内部表；
//! - [`TableOwner::Module`]：形状合规的模块表 `mod_<模块 id>_<后缀>`；
//! - [`TableOwner::Unattributed`]：长得像模块表、但**认不出是谁的**（id 不合规或后缀
//!   带了大写 / 连字符）——在 `modules.db` 里出现就是异常；
//! - [`TableOwner::Foreign`]：既不是核心表也不是模块表的名字——同样不该出现在这个库里。
//!
//! 后两种合起来就是「谁也不认的表」；体检（[`ModuleData::audit`](crate::store::ModuleData::audit)）
//! 把它们数出来给作者看，但**绝不自动删**（现场要留着）。

use yanmo_proto::is_valid_module_id;

/// 核心自己在 `modules.db` 里的记账表（**不是任何模块的数据**）。
///
/// 加一张核心表就加一项：漏了它，体检会把核心自己的账当成「谁也不认的表」。
pub const CORE_TABLES: [&str; 8] = [
    "module_schema",
    "module_data_health",
    "module_data_isolation",
    "module_usage",
    "module_read_consent",
    "module_run_log",
    "module_chain_cut",
    "module_backoff",
];

/// 一张表归谁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableOwner {
    /// 核心自己的记账表。
    Core,
    /// 某个模块的表：`module_id` + `suffix`。
    Module {
        /// 表名前缀里认出来的模块 id。
        module_id: String,
        /// `mod_<id>_` 之后的那一段。
        suffix: String,
    },
    /// 长得像模块表，但 id 或后缀不合规。
    Unattributed,
    /// 既不是核心表也不是模块表。
    Foreign,
}

/// 认一张表的归属（纯函数，不碰数据库）。
pub fn classify_table(table: &str) -> TableOwner {
    if table.starts_with("sqlite_") || CORE_TABLES.contains(&table) {
        return TableOwner::Core;
    }
    let Some(rest) = table.strip_prefix("mod_") else {
        return TableOwner::Foreign;
    };
    let Some((module_id, suffix)) = rest.split_once('_') else {
        return TableOwner::Unattributed;
    };
    if is_valid_module_id(module_id) && valid_suffix(suffix) {
        TableOwner::Module { module_id: module_id.to_string(), suffix: suffix.to_string() }
    } else {
        TableOwner::Unattributed
    }
}

/// 表后缀的字符集：非空，只认小写字母、数字与下划线。
///
/// 与写入口 [`ModuleData::is_module_table`](crate::store::ModuleData::is_module_table)
/// 同一口径——宽一格，就会出现「核心认不出、模块却以为自己写了」的表。
pub fn valid_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_the_first_underscore_after_mod() {
        assert_eq!(
            classify_table("mod_app.example.map_places"),
            TableOwner::Module {
                module_id: "app.example.map".to_string(),
                suffix: "places".to_string()
            }
        );
        // 后缀自己可以带下划线
        assert_eq!(
            classify_table("mod_app.example.map_grid_cells"),
            TableOwner::Module {
                module_id: "app.example.map".to_string(),
                suffix: "grid_cells".to_string()
            }
        );
    }

    #[test]
    fn core_tables_are_never_module_data() {
        for table in CORE_TABLES {
            assert_eq!(classify_table(table), TableOwner::Core, "{table}");
        }
        assert_eq!(classify_table("sqlite_sequence"), TableOwner::Core);
    }

    #[test]
    fn bad_ids_and_bad_suffixes_are_unattributed() {
        for table in [
            "mod_app.example.map_",          // 空后缀
            "mod_app.example.map_Places",    // 大写
            "mod_app.example.map_bad-name",  // 连字符
            "mod_map_places",                // id 只有一段
            "mod_App.example.map_places",    // id 有大小写
            "mod_",                          // 什么都没有
        ] {
            assert_eq!(classify_table(table), TableOwner::Unattributed, "{table}");
        }
    }

    #[test]
    fn other_names_are_foreign() {
        for table in ["plain_table", "node_fts_data"] {
            assert_eq!(classify_table(table), TableOwner::Foreign, "{table}");
        }
        // 只有 `mod_` 头、后面没有下划线：认不出是谁的，不是「外来表」
        assert_eq!(classify_table("mod_x"), TableOwner::Unattributed);
    }

    #[test]
    fn suffix_charset_matches_the_write_gate() {
        assert!(valid_suffix("kv"));
        assert!(valid_suffix("grid_cells_2"));
        assert!(!valid_suffix(""));
        assert!(!valid_suffix("Kv"));
        assert!(!valid_suffix("kv-2"));
    }
}
