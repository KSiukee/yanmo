//! 壳层错误：**只给界面「码 + 参数」**，不产出人类句子。
//!
//! 核心错误（[`yanmo_core::Error`]）到这里被翻成同一形状；壳自己的失败
//! （数据目录、导出落盘）也用同一形状——界面只有**一条渲染路径**：
//! 拿 `code` 查字典、用 `params` 填模板；`detail` 只写日志。
//!
//! 这么做的理由与核心侧一致：界面文案得集中在一处（`frontend/src/locales/`），
//! 谁都不许在业务代码里随手拼一句中文丢给用户。

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// 壳自己产生的错误码 + **每个码允许带哪些参数**（核心那批在 [`yanmo_core::error::ERROR_CODES`]）。
///
/// 与核心码表同一条纪律：界面上那句文案的占位符，只能是这里声明的名字；
/// 调用点给的参数也必须正好是这些——两头对不上，界面就会显示成 `{path}` 而没人发现。
/// 加一个码就在这儿加一行（有穷举测试盯着，见本文件末尾）。
pub const SHELL_CODES: &[(&str, &[&str])] = &[
    ("shell.data_dir_unavailable", &[]),
    ("shell.data_dir_readonly", &["path"]),
    ("shell.recorded_dir_missing_db", &["path", "pointer"]),
    ("shell.relocate_pending_missing", &[]),
    ("shell.data_dir_create_failed", &["path"]),
    ("shell.db_open_failed", &["path"]),
    ("shell.session_begin_failed", &[]),
    ("shell.store_unavailable", &[]),
    ("shell.open_dir_failed", &["path"]),
    ("shell.store_closed", &[]),
    ("shell.store_reopen_failed", &["path"]),
    ("shell.store_reopen_missing", &["path"]),
    ("shell.restore_left_without_library", &["path"]),
    ("shell.already_running", &["path"]),
    ("shell.export_dir_unreadable", &["path"]),
    ("shell.export_file_remove_failed", &["path"]),
    ("shell.export_dir_create_failed", &["path"]),
    ("shell.export_write_failed", &["path"]),
];

/// 壳层失败**人人都有的那个参数**：底层原因（数据库 / 文件系统给的技术消息）。
///
/// 它由 [`ApiError::caused_by`] 统一补上，所以不必逐码声明；
/// 界面字典里写 `{detail}` 就一定能填上——这正是原先漏掉的那一环。
pub const SHELL_DETAIL: &str = "detail";

/// 取某个壳层码声明的参数名（没登记就是 `None`）。
fn shell_code_params(code: &str) -> Option<&'static [&'static str]> {
    SHELL_CODES.iter().find(|(name, _)| *name == code).map(|(_, params)| *params)
}

/// 一次失败的**界面形状**：`code` + `params` 给界面，`detail` 给日志。
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub params: BTreeMap<String, String>,
    /// 日志与开发者看的细节——**界面不许拿它当文案**（它多半是英文技术消息）
    pub detail: String,
}

impl ApiError {
    /// 没有附加参数的失败。
    pub fn new(code: &'static str) -> Self {
        Self::with(code, [])
    }

    /// 带参数的失败：参数只放**取值**，别塞成品句子。
    pub fn with(
        code: &'static str,
        params: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
        debug_assert!(SHELL_CODES.iter().any(|(name, _)| *name == code), "壳层错误码没登记进 SHELL_CODES：{code}");
        let params: BTreeMap<String, String> =
            params.into_iter().map(|(name, value)| (name.to_string(), value)).collect();
        if let Some(declared) = shell_code_params(code) {
            let missing: Vec<&str> =
                declared.iter().copied().filter(|name| !params.contains_key(*name)).collect();
            let extra: Vec<&str> = params
                .keys()
                .map(String::as_str)
                .filter(|name| !declared.contains(name) && *name != SHELL_DETAIL)
                .collect();
            // i18n-allow-next-line: 开发者断言（崩在测试/调试构建里），不进界面
            debug_assert!(
                missing.is_empty() && extra.is_empty(),
                "壳层错误码 {code} 的参数与码表声明对不上：少了 {missing:?}、多了 {extra:?}，码表写的是 {declared:?}"
            );
        }
        Self { code: code.to_string(), params: params.clone(), detail: render(code, &params) }
    }

    /// 追一句底层原因：**既进日志、也作为 `detail` 参数交给界面**。
    ///
    /// 为什么要给界面：像「无法创建数据目录 {path}：{detail}」这样的句子，
    /// 真正有用的信息（磁盘满 / 没权限）就在 `detail` 里——只写日志的话，
    /// 界面上会剩下一个没填上的 `{detail}`，那是最典型的一种显示崩坏。
    pub fn caused_by(mut self, cause: impl fmt::Display) -> Self {
        let cause = cause.to_string();
        self.detail = format!("{} | {cause}", self.detail);
        self.params.insert(SHELL_DETAIL.to_string(), cause);
        self
    }
}

fn render(code: &str, params: &BTreeMap<String, String>) -> String {
    if params.is_empty() {
        return code.to_string();
    }
    let filled: Vec<String> = params.iter().map(|(name, value)| format!("{name}={value}")).collect();
    format!("{code}({})", filled.join(", "))
}

impl From<yanmo_core::Error> for ApiError {
    fn from(e: yanmo_core::Error) -> Self {
        let params: BTreeMap<String, String> = e
            .params()
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect();
        Self { code: e.code().to_string(), params, detail: e.to_string() }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

/// 读界面字典（测试用）。
fn dictionary() -> BTreeMap<String, String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/locales/zh-Hans.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取界面字典 {} 失败：{e}", path.display()));
    serde_json::from_str(&text).expect("界面字典应当是合法的「键 → 文案」JSON")
}

/// 文案里的占位符名字（`{work_id}` → `work_id`）。
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}') else { break };
        out.push(rest[start + 1..start + end].to_string());
        rest = &rest[start + end + 1..];
    }
    out
}

    #[test]
    fn core_failures_keep_their_code_and_params() {
        let core = yanmo_core::Error::invalid_with(
            yanmo_core::error::codes::WORK_NOT_TRASHED,
            [("work_id", "7".to_string())],
        );
        let api = ApiError::from(core);
        assert_eq!(api.code, "work.not_trashed");
        assert_eq!(api.params.get("work_id").map(String::as_str), Some("7"));
        assert!(api.detail.contains("回收站里没有这本书"), "detail 是给日志的：{}", api.detail);
    }

    /// `caused_by` 必须**同时**把底层原因交给界面：只在日志里留一份，
    /// 界面上就会显示成「无法创建目录 X：{detail}」。
    #[test]
    fn caused_by_hands_the_reason_to_the_ui_too() {
        let api = ApiError::with("shell.data_dir_create_failed", [("path", "稿子目录".into())])
            .caused_by(std::io::Error::other("磁盘满"));
        assert_eq!(api.params.get("detail").map(String::as_str), Some("磁盘满"));
        assert!(api.detail.contains("磁盘满"), "日志里也该有：{}", api.detail);
    }

    /// 穷举对表：**核心与壳的每个码都必须在界面字典里查得到**，反之也不许有孤儿键。
    ///
    /// 这是「码表穷举测试」——漏一个码，界面上就只剩「未知错误」，而那是最难查的一类故障。
    #[test]
    fn every_error_code_has_a_dictionary_entry() {
        let dict = dictionary();
        let known: std::collections::BTreeSet<String> = yanmo_core::error::ERROR_CODES
            .iter()
            .copied()
            .chain(SHELL_CODES.iter().map(|(code, _)| *code))
            .map(|code| format!("error.{code}"))
            .collect();
        assert!(known.len() > 20, "码表小得可疑，检查扫描基准");

        for key in &known {
            let text = dict.get(key).unwrap_or_else(|| {
                panic!("错误码 {key} 在界面字典里查不到——界面上会显示成「未知错误」")
            });
            assert!(!text.trim().is_empty(), "字典里的 {key} 是空的");
        }
        for key in dict.keys().filter(|key| key.starts_with("error.")) {
            assert!(
                known.contains(key),
                "字典里的 {key} 没有对应错误码——码表与字典各说各话"
            );
        }
    }

    /// **占位符对表**：字典里那句文案的占位符，必须是码表为这个码声明的参数名。
    ///
    /// 这条守的是「改个参数名，界面静默显示成 `{node_id}`」——那属于最难查的一类显示崩坏：
    /// 不报错、不空白，只是把内部名字露给作者看。
    #[test]
    fn dictionary_placeholders_match_the_declared_params() {
        let dict = dictionary();
        let mut checked = 0;
        for code in yanmo_core::error::ERROR_CODES {
            let declared = yanmo_core::error_codes::code_params(code)
                .unwrap_or_else(|| panic!("码 {code} 没在码表里声明参数名"));
            for name in placeholders(&dict[&format!("error.{code}")]) {
                assert!(
                    declared.contains(&name.as_str()),
                    "字典里的 error.{code} 用了 {{{name}}}，但码表只声明了 {declared:?}——界面上会永远显示成 {{{name}}}"
                );
                checked += 1;
            }
        }
        for (code, declared) in SHELL_CODES {
            // 壳层的 detail 是公共尾巴（caused_by 补），字典里写它就一定能填上
            let allowed: Vec<&str> =
                declared.iter().copied().chain(std::iter::once(SHELL_DETAIL)).collect();
            for name in placeholders(&dict[&format!("error.{code}")]) {
                assert!(
                    allowed.contains(&name.as_str()),
                    "字典里的 error.{code} 用了 {{{name}}}，但壳层码表只声明了 {allowed:?}"
                );
                checked += 1;
            }
        }
        assert!(checked > 20, "一个占位符都没检查到，这条测试会假绿");
    }

    /// 壳里的全部源码（`src/**/*.rs`），**排除 `error.rs`**——那份是码表声明处：
    /// 把它算进来，任何码都能"自己证明自己有人用"，这条守卫就假绿了。
    fn shell_sources() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).expect("读壳源码目录失败").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs")
                    && path.file_name().is_some_and(|name| name != "error.rs")
                {
                    out.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap_or_default()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
        assert!(out.len() > 3, "没扫到壳源码，这条守卫会假绿");
        out
    }

    /// 壳层码：既要有调用点，**字典里写了 `{detail}` 的还必须接上 `caused_by`**。
    ///
    /// 这一条守的是最容易漏的一处：文案里有 `{detail}`、调用点却忘了把底层原因交上来，
    /// 界面就会显示成「无法创建目录 {path}：{detail}」——不报错，只是把内部占位符露给作者。
    ///
    /// 扫的是**整个壳**（不只 `storage.rs`）：命令域里的失败同样要守这条规矩。
    #[test]
    fn every_shell_code_is_used_and_hands_over_its_reason() {
        let sources = shell_sources();
        let dict = dictionary();
        for (code, _) in SHELL_CODES {
            let needle = format!("{code}\"");
            let mut hit: Option<(usize, usize)> = None;
            for (index, (_, text)) in sources.iter().enumerate() {
                if let Some(at) = text.find(&needle) {
                    hit = Some((index, at));
                    break;
                }
            }
            let (index, at) = hit
                .unwrap_or_else(|| panic!("壳层错误码 {code} 没人用——要么补上，要么从码表里删掉"));
            let (where_, text) = &sources[index];
            // 取这一句（到分号为止）看它有没有接上底层原因。
            // 注意按**字符**截断：按字节切会把中文切成半个字（UTF-8 边界）
            let tail: String = text[at..].chars().take(400).collect();
            let statement = tail.split(';').next().unwrap_or(&tail);
            if dict[&format!("error.{code}")].contains("{detail}") {
                assert!(
                    statement.contains("caused_by"),
                    "error.{code} 的文案里有 {{detail}}，但 {where_} 的调用点没接 caused_by——界面上会显示成 {{detail}}"
                );
            }
        }
    }

    /// **排版规则对表**：核心给的每条规则代码，界面字典里都得有名字与说明。
    ///
    /// 漏一条的后果与错误码漏登记同款：界面上那一行只剩一个 `typeset.rule.xxx`，
    /// 作者看不懂那条建议到底要改什么。规则清单以核心的 `typeset::RULES` 为准，
    /// 界面不另抄一份"有哪些规则"。
    #[test]
    fn every_typeset_rule_has_a_dictionary_entry() {
        let dict = dictionary();
        let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for rule in yanmo_core::typeset::RULES {
            assert!(
                ["safe", "careful", "style"].contains(&rule.tier),
                "规则 {} 的风险档 {} 不认识——界面不知道该不该默认勾上",
                rule.code,
                rule.tier
            );
            let key = format!("typeset.rule.{}", rule.code);
            let text = dict
                .get(&key)
                .unwrap_or_else(|| panic!("排版规则 {} 在界面字典里查不到（缺 {key}）", rule.code));
            assert!(!text.trim().is_empty(), "字典里的 {key} 是空的");
            known.insert(key);
        }
        assert!(known.len() >= 5, "规则少得可疑，检查扫描基准");
        for key in dict.keys().filter(|key| key.starts_with("typeset.rule.")) {
            assert!(known.contains(key), "字典里的 {key} 没有对应规则——两边各说各话");
        }
        // 「只报告不给改法」的检查同样要对得上表（漏一条＝界面上只剩一个码）
        for code in yanmo_core::typeset::NOTICE_RULES {
            let key = format!("typeset.notice.{code}");
            let text = dict
                .get(&key)
                .unwrap_or_else(|| panic!("提醒检查 {code} 在界面字典里查不到（缺 {key}）"));
            assert!(!text.trim().is_empty(), "字典里的 {key} 是空的");
        }
    }

    /// **编译预设对表**：核心给的每个预设代码，界面字典里都得有名字。
    ///
    /// 漏一条的后果与规则漏登记同款：界面上那个选项只剩一个 `compile.preset.xxx`。
    #[test]
    fn every_compile_preset_has_a_dictionary_entry() {
        let dict = dictionary();
        for preset in yanmo_core::compile::Preset::ALL {
            let key = format!("compile.preset.{}", preset.as_str());
            let text = dict
                .get(&key)
                .unwrap_or_else(|| panic!("编译预设 {} 在界面字典里查不到（缺 {key}）", preset.as_str()));
            assert!(!text.trim().is_empty(), "字典里的 {key} 是空的");
        }
    }
}
