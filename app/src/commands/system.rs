//! 系统类命令：引擎与环境信息（不含任何正文读写）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;

/// 引擎基本信息。
///
/// 存在的意义：验证「壳 → 核心」链路已打通（`yanmo-app` → `yanmo-core` / `yanmo-proto`）。
#[derive(Debug, Serialize)]
pub struct EngineInfo {
    /// 产品名
    pub name: &'static str,
    /// 引擎版本，来自 [`yanmo_core::engine_version`]
    pub version: &'static str,
    /// 本机协议版本，来自 [`yanmo_proto::PROTOCOL_VERSION`]
    pub protocol_version: u32,
    /// 数据格式版本，来自 [`yanmo_core::version::DATA_FORMAT_VERSION`]
    pub data_format_version: u32,
}

#[tauri::command]
pub fn engine_info() -> EngineInfo {
    EngineInfo {
        // i18n-allow-next-line: 产品名（品牌），不翻译
        name: "研墨",
        version: yanmo_core::engine_version(),
        protocol_version: yanmo_proto::PROTOCOL_VERSION,
        data_format_version: yanmo_core::version::DATA_FORMAT_VERSION,
    }
}

/// 稿子落在哪里。
///
/// 路径由壳解析后**只读报告**给界面——用户有权知道自己的稿子在哪；
/// 但界面既不能指定它，也不能改写它（这里没有写入入口）。
#[derive(Debug, Serialize)]
pub struct DataHome {
    /// 数据库文件的绝对路径（仅供展示）
    pub path: String,
    /// 库内实际的数据结构版本
    pub schema_version: u32,
}

#[tauri::command]
pub fn data_home(data: State<'_, AppData>) -> Result<DataHome, ApiError> {
    Ok(DataHome {
        path: data.db_path().display().to_string(),
        schema_version: data.schema_version()?,
    })
}

/// 退出应用。
///
/// **只由退出闸门通过后调用**：关窗那一步已经先落盘、并留好了关窗快照；
/// 这里不再做任何判断，免得两处逻辑各说各话。顺手告诉兜底时钟"退出已开始"。
#[tauri::command]
pub fn exit_app(app: tauri::AppHandle, data: State<'_, AppData>) {
    data.exit_watch().mark_exiting();
    app.exit(0);
}
