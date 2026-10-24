//! 模块宿主的本机 IPC **数据形状**：握手 / 请求 / 应答 / 帧 / 能力码。
//!
//! 传输（回环 TCP、客户端、服务端）在独立的 `yanmo-ipc` crate 里——**核心与协议层
//! 不含 `std::net`**，所以"出货零出网"的边界不被模块平台破掉。这里只有：
//!
//! - 有界帧（u32 大端长度 + JSON，≤ 1 MiB）；
//! - 握手 / 请求 / 应答的 serde 类型；
//! - [`ReadHandler`]：核心侧实现它，传输层不需要知道 `Store`；
//! - 能力码 ↔ [`CoreDataArea`] 的解析。
//!
//! 一次连接只服务一个宿主、只绑回环；一次性模式 token 用一次，常驻模式
//! 同一启动器内可重连（token 只在内存/回环）——这些在 `yanmo-ipc` 的
//! `serve_one` / `Client` 里落。

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::capability::CoreDataArea;

/// 单帧上限（请求与响应各算）。
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// 一条连接最多处理多少个请求。
pub const MAX_REQUESTS: usize = 256;
/// 握手 / 单次读写的超时。
pub const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// IPC 的稳定错误（码是给日志 / 宿主界面用的；`detail` 只给开发者）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcError {
    pub code: String,
    pub detail: String,
}

impl IpcError {
    pub const IO: &'static str = "ipc.io";
    pub const TIMEOUT: &'static str = "ipc.timeout";
    pub const PROTOCOL: &'static str = "ipc.protocol";
    pub const FRAME: &'static str = "ipc.frame_too_large";
    pub const HANDSHAKE: &'static str = "ipc.handshake_failed";
    pub const UNKNOWN_CAP: &'static str = "ipc.unknown_capability";
    pub const RESPONSE_TOO_LARGE: &'static str = "ipc.response_too_large";
    pub const TOO_MANY: &'static str = "ipc.too_many_requests";
    pub const NOT_LOOPBACK: &'static str = "ipc.not_loopback";
    pub const BAD_ARGUMENT: &'static str = "ipc.bad_argument";
    pub const UNKNOWN_MODULE: &'static str = "ipc.unknown_module";

    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { code: code.into(), detail: detail.into() }
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for IpcError {}

impl From<std::io::Error> for IpcError {
    fn from(error: std::io::Error) -> Self {
        let code = match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Self::TIMEOUT,
            _ => Self::IO,
        };
        Self::new(code, error.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(Self::PROTOCOL, error.to_string())
    }
}

/// 握手。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub token: String,
}

/// 握手应答。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeAck {
    pub ok: bool,
    pub code: Option<String>,
}

/// 一次读取请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub capability: String,
    pub arg: Option<String>,
    /// 多模块宿主：这次读属于哪个模块；单模块模式为 `None`。
    #[serde(default)]
    pub module_id: Option<String>,
}

/// 一次读取应答。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    pub data: Option<String>,
    pub code: Option<String>,
    pub detail: Option<String>,
}

/// 核心侧的实现：按区域读数据，返回 **JSON 字符串**（不是任意二进制）。
pub trait ReadHandler {
    fn read(&mut self, area: CoreDataArea, arg: Option<&str>) -> Result<String, IpcError>;
}

/// 把稳定码解析回区域。
pub fn parse_capability(code: &str) -> Option<CoreDataArea> {
    CoreDataArea::ALL.into_iter().find(|area| area.as_str() == code)
}

/// 写一帧：u32 大端长度 + JSON 字节。
pub fn write_frame(stream: &mut impl Write, bytes: &[u8]) -> Result<(), IpcError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(IpcError::new(IpcError::FRAME, format!("{} bytes", bytes.len())));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes()).map_err(IpcError::from)?;
    stream.write_all(bytes).map_err(IpcError::from)?;
    stream.flush().map_err(IpcError::from)
}

/// 读一帧；EOF 是错误（客户端不该在没有应答时断）。
pub fn read_frame(stream: &mut impl Read) -> Result<Vec<u8>, IpcError> {
    read_frame_opt(stream)?.ok_or_else(|| IpcError::new(IpcError::IO, "unexpected eof"))
}

/// 读一帧；对端干净关闭（没有新帧）返回 `Ok(None)`，服务端据此收工。
pub fn read_frame_opt(stream: &mut impl Read) -> Result<Option<Vec<u8>>, IpcError> {
    let mut header = [0u8; 4];
    match stream.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(IpcError::from(error)),
    }
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(IpcError::new(IpcError::FRAME, format!("{length} bytes")));
    }
    let mut buffer = vec![0u8; length];
    stream.read_exact(&mut buffer).map_err(IpcError::from)?;
    Ok(Some(buffer))
}
