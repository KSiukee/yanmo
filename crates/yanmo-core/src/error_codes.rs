//! 错误码表：核心面向界面的**唯一"话"清单**。
//!
//! 规矩只有三条，但必须一直守：
//!
//! 1. **核心只给「码 + 参数」**——码是点分小写的英文标识（`work.title_empty`），参数是键值对；
//!    界面拿码去查自己的字典渲染成句子。核心**不生产界面文案**（这是"核心零 UI 依赖"的一部分）。
//! 2. **参数里只放取值，不放成品句子**——`{work_id}` 可以，`"这本书没了"` 不行。
//! 3. **加了新码就在这里加一行**：常量、`ERROR_CODES`、日志模板、**参数名清单**四样由下面的
//!    宏一次性生成，漏不掉；`ERROR_CODES` 与参数清单是界面字典的对表基准（有穷举测试盯着：
//!    字典里的占位符只能是这个码声明的参数名）。
//!
//! # 为什么还留着中文模板
//!
//! `Display` 是给**日志与开发者**看的，不是给界面看的——日志里一句中文比一串码好读。
//! 所以码表里额外带一份中文模板；界面文案一律在界面字典里，两者互不冒充。
// i18n-allow-file: 本文件是「日志模板」清单（只服务日志与开发者）；界面文案在 locales/zh-Hans.json

/// 错误参数：`(名字, 取值)`。名字与日志模板里的 `{名字}` 对应。
pub type Params = Vec<(&'static str, String)>;

macro_rules! error_codes {
    ($( $name:ident => $code:literal : $log:literal : [$($param:literal),* $(,)?] ),* $(,)?) => {
        /// 错误码常量——调用点一律用常量，不写字面量（写错在 debug 构建里会当场断言）。
        pub mod codes {
            $( pub const $name: &str = $code; )*
        }

        /// 全部错误码：界面字典必须一个不漏地有对应条目（穷举测试拿它对表）。
        pub const ERROR_CODES: &[&str] = &[ $($code),* ];

        /// 码 → **这个码允许带哪些参数**。
        ///
        /// 它是「核心与界面之间的接口」：界面字典里那句文案的占位符，只能是这些名字；
        /// 调用点给的参数也必须正好是这些（多给少给在 debug 构建里当场断言）。
        /// 少了这道校验，改个参数名就会让界面显示成 `{node_id}` 而没人发现。
        pub fn code_params(code: &str) -> Option<&'static [&'static str]> {
            match code {
                $( $code => Some(&[$($param),*]), )*
                _ => None,
            }
        }

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
DB => "db" : "数据库错误：{detail}" : ["detail"],
    SCHEMA_TOO_NEW => "schema_too_new"
        : "数据格式版本 {found} 高于本引擎支持的 {supported}——请升级研墨，不要用旧版打开新库" : ["found", "supported"],
    IO => "io" : "文件读写失败：{detail}" : ["detail"],
    WAL_UNAVAILABLE => "env.wal_unavailable"
        : "无法启用 WAL（当前模式 {mode}）——网络盘/只读目录不支持" : ["mode"],
    SQLITE_TOO_OLD => "env.sqlite_too_old"
        : "SQLite {version} 过旧（需 >= 3.34，trigram 分词要用）" : ["version"],
    FTS5_UNAVAILABLE => "env.fts5_unavailable"
        : "FTS5 不可用（全文检索会退化成扫描）：{detail}" : ["detail"],
    PATH_NO_PARENT => "file.path_no_parent" : "路径没有上级目录：{path}" : ["path"],
    UNKNOWN_NODE_KIND => "value.unknown_node_kind" : "未知的节点类型：{value}" : ["value"],
    UNKNOWN_WORK_KIND => "value.unknown_work_kind" : "未知的作品类型：{value}" : ["value"],
    UNKNOWN_WORK_LANGUAGE => "value.unknown_work_language" : "未知的作品语言：{value}" : ["value"],
    UNKNOWN_QUESTION_STATE => "value.unknown_question_state"
        : "未知的问题卡状态：{value}" : ["value"],
    BACKUP_SNAPSHOT_FAILED => "backup.snapshot_failed"
        : "做一致性快照失败（库可能有问题）：{detail}" : ["detail"],
    BACKUP_RESTORE_SOURCE_INVALID => "backup.restore_source_invalid"
        : "这个位置不是一个备份包或库文件：{path}" : ["path"],
    BACKUP_RESTORE_BLOCKED => "backup.restore_blocked"
        : "这份备份不能用来恢复：{reason}" : ["reason"],
    BACKUP_RESTORE_SWAP_FAILED => "backup.restore_swap_failed"
        : "换库没成功（原库已放回原处）：{detail}" : ["detail"],
    IMPORT_DRAFT_INVALID => "import.draft_invalid"
        : "这份成稿 JSON 不合格（{field}）：{detail}" : ["field", "detail"],
    UNKNOWN_WORD_CALIBER => "value.unknown_word_caliber"
        : "未知的字数口径：{value}（只有 chars / chars_no_punct / words）" : ["value"],
    UNKNOWN_EXPORT_FORMAT => "value.unknown_export_format"
        : "不认识的导出格式：{value}（只支持 txt / json）" : ["value"],
    UNKNOWN_COMPILE_PRESET => "value.unknown_compile_preset"
        : "不认识的编译预设：{value}" : ["value"],
    UNKNOWN_NAMING_STYLE => "value.unknown_naming_style" : "不认识的命名规则：{value}" : ["value"],
    UNKNOWN_CHAPTER_NUMBERING => "value.unknown_chapter_numbering"
        : "不认识的章节编号方式：{value}（只有 continue / per_volume）" : ["value"],
    UNKNOWN_QUOTE_STYLE => "value.unknown_quote_style"
        : "不认识的引号风格：{value}（只有 curly / corner）" : ["value"],
    TYPESET_CHANGE_UNKNOWN => "typeset.change_unknown"
        : "要应用的排版改动（第 {index} 处）不在这份稿子里——稿子可能已经改过了，请重新预览"
        : ["index"],
    WORK_TITLE_EMPTY => "work.title_empty" : "作品标题不能为空" : [],
    WORK_NOT_FOUND => "work.not_found" : "作品不存在：{work_id}" : ["work_id"],
    WORK_GONE => "work.gone" : "作品不存在或已删除：{work_id}" : ["work_id"],
    WORK_NOT_TRASHED => "work.not_trashed" : "回收站里没有这本书：{work_id}" : ["work_id"],
    NODE_NOT_FOUND => "node.not_found" : "节点不存在：{node_id}" : ["node_id"],
    NODE_GONE => "node.gone" : "节点不存在或已删除：{node_id}" : ["node_id"],
    NODE_NOT_TRASHED => "node.not_trashed" : "回收站里没有这一段：{node_id}" : ["node_id"],
    NODE_NOT_BODY => "node.not_body" : "节点不承载正文，不能当章节导航：{node_id}" : ["node_id"],
    NODE_NOT_EDITABLE => "node.not_editable"
        : "不能切到该节点（不存在 / 已删除 / 不承载正文）：{node_id}" : ["node_id"],
    NODE_FOREIGN_PARENT => "node.foreign_parent"
        : "节点 {node_id} 属于作品 {owner}，不能挂到作品 {work_id} 下" : ["node_id", "owner", "work_id"],
    CARD_NOT_FOUND => "card.not_found" : "问题卡不存在：{card_id}" : ["card_id"],
    CARD_BODY_EMPTY => "card.body_empty" : "问题卡的内容不能为空" : [],
    CARD_IMPORTANCE_OUT_OF_RANGE => "card.importance_out_of_range"
        : "问题卡的重要度要落在 0~1：{value}" : ["value"],
    CARD_DERIVED_FROM_INVALID => "card.derived_from_invalid"
        : "派生来源对不上：{derived_from} 不是这本书（{work_id}）里的一张卡"
        : ["derived_from", "work_id"],
    CARD_ILLEGAL_TRANSITION => "card.illegal_transition"
        : "问题卡不能从 {from} 走到 {to}——这不是生命周期里的合法一条边" : ["from", "to"],
    CARD_AUTO_DERIVED_NEEDS_SOURCE => "card.auto_derived_needs_source"
        : "自动派生的问题必须说清它是从哪张卡派生的" : [],
    CARD_DERIVATION_TOO_DEEP => "card.derivation_too_deep"
        : "自动派生已经到链深上限（最多 {max} 层）——作者自己顺着灵感再问不受此限" : ["max"],
    TREE_TOO_DEEP => "tree.too_deep"
        : "节点树超过深度上限（最多 {max} 层），已拒绝继续" : ["max"],
    TREE_CYCLE_SUSPECTED => "tree.cycle_suspected" : "节点树深度异常（疑似成环），已拒绝继续" : [],
    TREE_MOVE_INTO_DESCENDANT => "tree.move_into_descendant"
        : "不能把节点移进自己的子孙里——那会形成环" : [],
    VOLUME_CLOSE_POINT => "volume.close_point"
        : "这里收不了卷：收卷点要落在一章上，且它所在的那一层是卷或根" : [],
    VOLUME_NOT_VOLUME => "volume.not_volume"
        : "这个节点不是卷，撤不了卷：{node_id}" : ["node_id"],
    TRASH_PURGE_NEEDS_TRASHED => "trash.purge_needs_trashed"
        : "只能彻底删除已经在回收站里的东西：{id}" : ["id"],
    SNAPSHOT_NOT_FOUND => "snapshot.not_found"
        : "版本快照不存在：{snapshot_id}" : ["snapshot_id"],
    LOCATION_RECORD_WRITE_FAILED => "location.record_write_failed"
        : "记不下稿子的位置（{path}）：{detail}" : ["path", "detail"],
    STORE_RELOCATE_SOURCE_MISSING => "store.relocate_source_missing"
        : "原目录里找不到稿子库：{path}" : ["path"],
    STORE_MISSING => "store.missing"
        : "这个位置没有稿库：{path}（库文件叫 yanmo.db）。命令行不会替你新建一个空库——先用研墨打开一次，或确认路径写对了"
        : ["path"],
    STORE_RELOCATE_INSIDE => "store.relocate_inside"
        : "新位置在现在的稿子目录里面（{from} → {to}），不能往自己里面搬" : ["from", "to"],
    STORE_RELOCATE_TARGET_IN_USE => "store.relocate_target_in_use"
        : "新位置里已经有一份稿子：{path}（换个空目录，别让两份稿子混在一起）" : ["path"],
    STORE_RELOCATE_COPY_FAILED => "store.relocate_copy_failed"
        : "往新位置复制稿子失败（{path}）：{detail}" : ["path", "detail"],
    STORE_RELOCATE_VERIFY_FAILED => "store.relocate_verify_failed"
        : "新位置复制过去的稿子核对不上（{path}）：原位置没动过，稿子还在原处（{detail}）"
        : ["path", "detail"],
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

    /// 声明的参数名必须与日志模板里的占位符**正好相等**——
    /// 多了会在日志里留下没填上的 `{xxx}`，少了则是模板写漏了参数。
    #[test]
    fn declared_params_match_the_log_template() {
        for code in ERROR_CODES {
            let declared =
                code_params(code).unwrap_or_else(|| panic!("码 {code} 没在码表里声明参数名"));
            let template = log_template(code).expect("每个码都该有日志模板");
            let mut used = Vec::new();
            let mut rest = template;
            while let Some(start) = rest.find('{') {
                let Some(end) = rest[start..].find('}') else { break };
                used.push(&rest[start + 1..start + end]);
                rest = &rest[start + end + 1..];
            }
            used.sort_unstable();
            used.dedup();
            let mut expected: Vec<&str> = declared.to_vec();
            expected.sort_unstable();
            assert_eq!(
                used, expected,
                "码 {code} 的日志模板占位符与声明的参数名对不上（日志或界面会出现没填上的花括号）"
            );
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
