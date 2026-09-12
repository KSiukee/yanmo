//! 错误码表：核心面向界面的**唯一"话"清单**。
//!
//! 规矩只有三条，但必须一直守：
//!
//! 1. **核心只给「码 + 参数」**——码是点分小写的英文标识（`work.title_empty`），参数是键值对；
//!    界面拿码去查自己的字典渲染成句子。核心**不生产界面文案**（这是"核心零 UI 依赖"的一部分）。
//! 2. **参数里只放取值，不放成品句子**——`{work_id}` 可以，`"这本书没了"` 不行。
//! 3. **加了新码就在这里加一行**：常量、`ERROR_CODES`、日志模板三样由下面的宏一次性生成，
//!    漏不掉；`ERROR_CODES` 是界面字典的对表基准（有穷举测试盯着）。
//!
//! # 为什么还留着中文模板
//!
//! `Display` 是给**日志与开发者**看的，不是给界面看的——日志里一句中文比一串码好读。
//! 所以码表里额外带一份中文模板；界面文案一律在界面字典里，两者互不冒充。
// i18n-allow-file: 本文件是「日志模板」清单（只服务日志与开发者）；界面文案在 locales/zh-Hans.json

/// 错误参数：`(名字, 取值)`。名字与日志模板里的 `{名字}` 对应。
pub type Params = Vec<(&'static str, String)>;

macro_rules! error_codes {
    ($( $name:ident => $code:literal : $log:literal ),* $(,)?) => {
        /// 错误码常量——调用点一律用常量，不写字面量（写错在 debug 构建里会当场断言）。
        pub mod codes {
            $( pub const $name: &str = $code; )*
        }

        /// 全部错误码：界面字典必须一个不漏地有对应条目（穷举测试拿它对表）。
        pub const ERROR_CODES: &[&str] = &[ $($code),* ];

        /// 码 → 日志用中文模板（`{名字}` 由参数填）。
        pub(crate) fn log_template(code: &str) -> Option<&'static str> {
            match code {
                $( $code => Some($log), )*
                _ => None,
            }
        }
    };
}

error_codes! {
    DB => "db" : "数据库错误：{detail}",
    SCHEMA_TOO_NEW => "schema_too_new"
        : "数据格式版本 {found} 高于本引擎支持的 {supported}——请升级研墨，不要用旧版打开新库",
    IO => "io" : "文件读写失败：{detail}",

    WAL_UNAVAILABLE => "env.wal_unavailable"
        : "无法启用 WAL（当前模式 {mode}）——网络盘/只读目录不支持",
    SQLITE_TOO_OLD => "env.sqlite_too_old"
        : "SQLite {version} 过旧（需 >= 3.34，trigram 分词要用）",
    FTS5_UNAVAILABLE => "env.fts5_unavailable"
        : "FTS5 不可用（全文检索会退化成扫描）：{detail}",

    PATH_NO_PARENT => "file.path_no_parent" : "路径没有上级目录：{path}",

    UNKNOWN_NODE_KIND => "value.unknown_node_kind" : "未知的节点类型：{value}",
    UNKNOWN_WORK_KIND => "value.unknown_work_kind" : "未知的作品类型：{value}",
    UNKNOWN_EXPORT_FORMAT => "value.unknown_export_format"
        : "不认识的导出格式：{value}（只支持 txt / json）",
    UNKNOWN_GAP_ANSWER => "value.unknown_gap_answer" : "未知的答复：{value}",

    WORK_TITLE_EMPTY => "work.title_empty" : "作品标题不能为空",
    WORK_NOT_FOUND => "work.not_found" : "作品不存在：{work_id}",
    WORK_GONE => "work.gone" : "作品不存在或已删除：{work_id}",
    WORK_NOT_TRASHED => "work.not_trashed" : "回收站里没有这本书：{work_id}",

    NODE_NOT_FOUND => "node.not_found" : "节点不存在：{node_id}",
    NODE_GONE => "node.gone" : "节点不存在或已删除：{node_id}",
    NODE_NOT_TRASHED => "node.not_trashed" : "回收站里没有这一段：{node_id}",
    NODE_NOT_BODY => "node.not_body" : "节点不承载正文，不能当章节导航：{node_id}",
    NODE_NOT_EDITABLE => "node.not_editable"
        : "不能切到该节点（不存在 / 已删除 / 不承载正文）：{node_id}",
    NODE_FOREIGN_PARENT => "node.foreign_parent"
        : "节点 {node_id} 属于作品 {owner}，不能挂到作品 {work_id} 下",
    NODE_GAP_NO_SERIAL => "node.gap_no_serial" : "这一段没有编号可补：{title}",

    TREE_CYCLE_SUSPECTED => "tree.cycle_suspected" : "节点树深度异常（疑似成环），已拒绝继续",
    TREE_MOVE_INTO_DESCENDANT => "tree.move_into_descendant"
        : "不能把节点移进自己的子孙里——那会形成环",

    TRASH_PURGE_NEEDS_TRASHED => "trash.purge_needs_trashed"
        : "只能彻底删除已经在回收站里的东西：{id}",
}

/// 按码把参数填进日志模板；模板不认识这个码就退回 `码(名字=取值, …)`。
///
/// **只服务日志**：界面永远拿 `code()` + `params()` 自己渲染。
pub(crate) fn render_log(code: &str, params: &[(&'static str, String)]) -> String {
    let Some(template) = log_template(code) else {
        let filled: Vec<String> = params
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        return format!("{code}({})", filled.join(", "));
    };
    let mut out = template.to_string();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_log_template_and_no_duplicates() {
        assert!(!ERROR_CODES.is_empty(), "码表不该是空的");
        let mut seen = std::collections::BTreeSet::new();
        for code in ERROR_CODES {
            assert!(
                log_template(code).is_some_and(|text| !text.trim().is_empty()),
                "码 {code} 没有日志模板——日志里会只剩一个码，排查时会骂人"
            );
            assert!(
                code.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_'),
                "码 {code} 只该是小写英文 / 数字 + 点 + 下划线"
            );
            assert!(seen.insert(code), "码 {code} 重复登记了");
        }
    }

    #[test]
    fn render_fills_every_placeholder_it_is_given() {
        let filled = render_log(
            codes::NODE_FOREIGN_PARENT,
            &[("node_id", "7".into()), ("owner", "1".into()), ("work_id", "2".into())],
        );
        assert_eq!(filled, "节点 7 属于作品 1，不能挂到作品 2 下");
        assert!(!filled.contains('{'), "模板里的占位符必须都被填掉：{filled}");
    }

    #[test]
    fn unknown_code_falls_back_to_code_plus_params() {
        let filled = render_log("not.in.the.table", &[("id", "9".into())]);
        assert_eq!(filled, "not.in.the.table(id=9)");
    }
}
