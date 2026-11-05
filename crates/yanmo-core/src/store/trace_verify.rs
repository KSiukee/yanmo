//! 留痕包自带的那支校验脚本（Python 标准库，不需要装研墨）。
//!
//! 为什么单独成文件：脚本要能**原样**交给包外的人，放在 `.py` 里比塞进 Rust 字符串好维护；
//! 编译期 `include_str!` 进来，包里的字节与仓里这份逐字节一致。

/// 写进包里的 `verify.py` 原文。
pub const VERIFY_PY: &str = include_str!("trace_verify.py");
