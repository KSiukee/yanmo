//! 中文排版命令域：**先看后改**——先扫出一处处的建议，作者勾了哪几处才改哪几处。
//!
//! 规则与"该改哪一处"全在核心的 `typeset`（纯函数、可单测、可被别的壳驱动）：
//! 这里只做参数转换，**不碰库、不落盘**。正文由界面递进来（作者手上那份可能还没落盘），
//! 改完把正文交回界面——落盘照旧走编辑器那条保存通道，命令不另开一条写入路径。

use yanmo_core::typeset::{Options, Report, RuleInfo};

use crate::error::ApiError;

/// 规则清单（稳定代码 + 风险档）：界面据此排布勾选项；每条的名字与说明在界面字典里。
#[tauri::command(rename_all = "snake_case")]
pub fn typeset_rules() -> Vec<RuleInfo> {
    yanmo_core::typeset::RULES.to_vec()
}

/// 扫描（dry-run）：只报告"哪里会怎么改"（能改的）与"哪里缺一半"（只能提醒的），
/// **一个字都不动**。
#[tauri::command(rename_all = "snake_case")]
pub fn typeset_scan(text: String, options: Options) -> Result<Report, ApiError> {
    Ok(yanmo_core::typeset::scan(&text, &options)?)
}

/// 应用作者勾中的那几处。
///
/// 序号指的是上一次扫描结果里的下标：有一个对不上就**整批拒绝**（只改一半比不改更糟），
/// 由界面重新扫一遍再让作者决定。
#[tauri::command(rename_all = "snake_case")]
pub fn typeset_apply(
    text: String,
    options: Options,
    accepted: Vec<usize>,
) -> Result<String, ApiError> {
    Ok(yanmo_core::typeset::apply(&text, &options, &accepted)?)
}
