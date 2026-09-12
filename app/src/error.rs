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

/// 壳自己产生的错误码（核心那批在 [`yanmo_core::error::ERROR_CODES`]）。
///
/// 加一个就在这儿加一行——界面字典必须同步（有穷举测试盯着，见本文件末尾）。
pub const SHELL_CODES: &[&str] = &[
    "shell.data_dir_unavailable",
    "shell.data_dir_create_failed",
    "shell.db_open_failed",
    "shell.session_begin_failed",
    "shell.store_unavailable",
    "shell.export_dir_unreadable",
    "shell.export_file_remove_failed",
    "shell.export_dir_create_failed",
    "shell.export_write_failed",
];

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
        debug_assert!(SHELL_CODES.contains(&code), "壳层错误码没登记进 SHELL_CODES：{code}");
        let params: BTreeMap<String, String> =
            params.into_iter().map(|(name, value)| (name.to_string(), value)).collect();
        Self { code: code.to_string(), params: params.clone(), detail: render(code, &params) }
    }

    /// 追一句底层原因——**只进日志**。
    pub fn caused_by(mut self, cause: impl fmt::Display) -> Self {
        self.detail = format!("{} | {cause}", self.detail);
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

    /// 穷举对表：**核心与壳的每个码都必须在界面字典里查得到**，反之也不许有孤儿键。
    ///
    /// 这是「码表穷举测试」——漏一个码，界面上就只剩「未知错误」，而那是最难查的一类故障。
    #[test]
    fn every_error_code_has_a_dictionary_entry() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("frontend/src/locales/zh-Hans.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("读取界面字典 {} 失败：{e}", path.display()));
        let dict: BTreeMap<String, String> =
            serde_json::from_str(&text).expect("界面字典应当是合法的「键 → 文案」JSON");

        let known: std::collections::BTreeSet<String> = yanmo_core::error::ERROR_CODES
            .iter()
            .chain(SHELL_CODES.iter())
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

    #[test]
    fn every_shell_code_is_used_at_least_once() {
        // 壳的码表很短，孤儿码一眼能看出来：它们只该在 storage.rs 里用
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/storage.rs"),
        )
        .expect("读取壳源码失败");
        for code in SHELL_CODES {
            assert!(
                source.contains(&format!("{code}\"")),
                "壳层错误码 {code} 没人用——要么补上，要么从码表里删掉"
            );
        }
    }
}
