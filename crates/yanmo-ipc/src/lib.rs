//! 研墨模块宿主的本机 IPC **传输**：回环 TCP + 一次性/可重连 token + 长度前缀 JSON。
//!
//! # 为什么单独成 crate
//!
//! 核心与协议层（`yanmo-core` / `yanmo-proto`）必须保持 **零 `std::net`**——"出货代码
//! 零出网"是产品承诺。模块平台要一条本机回环通道，就把它放在这个**不进核心/壳依赖**
//! 的独立 crate 里：宿主用 [`Client`]，启动器用 [`serve_one`]；不编这个 crate 就没有
//! 这条通道（与宿主一样可编译期剥离）。
//!
//! # 形状
//!
//! 1. 启动器 `bind_loopback()` 拿一个只绑 127.0.0.1 的监听器；
//! 2. 宿主 `Client::connect` 第一帧发 `Handshake { token }`；
//! 3. 启动器验一次 token（不匹配回 `ok:false` 并断开）；
//! 4. 之后每帧 `Request { id, capability, arg }` → `Response { id, ok, data }`；
//! 5. [`serve_one`] 只服务**一个**宿主，返回后监听器销毁；常驻宿主用
//!    [`serve_repeated`] 在同一监听器/ token 上重连。
//!
//! 能力码 / 帧格式 / 错误码都在 `yanmo_proto::ipc` 里；这里只做 socket 与线程。

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use yanmo_proto::ipc as wire;
use yanmo_proto::ipc::{IpcError, ReadHandler};

pub use yanmo_proto::ipc::{Handshake, HandshakeAck, Request, Response};

/// 等宿主连上来的默认上限（宿主启动 + wasm 编译远小于它）。
pub const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);

/// 只绑回环的监听器（端口由系统给）。
pub fn bind_loopback() -> Result<TcpListener, IpcError> {
    TcpListener::bind("127.0.0.1:0").map_err(IpcError::from)
}

/// 一次连接只服务一个宿主：验 token、逐帧应答、EOF 收工。
///
/// `listener` 按值传入——函数返回时它被丢弃，**同一个 token 无法再开第二条连接**。
/// 等宿主最多 [`ACCEPT_TIMEOUT`]；没人连就返回 `ipc.timeout`，不会把启动器卡死。
pub fn serve_one<H: ReadHandler>(
    listener: TcpListener,
    token: String,
    handler: &mut H,
) -> Result<(), IpcError> {
    serve_one_with_timeout(listener, token, handler, ACCEPT_TIMEOUT)
}

/// 同 [`serve_one`]，但可以指定等宿主的上限（启动器/验收用）。
pub fn serve_one_with_timeout<H: ReadHandler>(
    listener: TcpListener,
    token: String,
    handler: &mut H,
    accept_timeout: Duration,
) -> Result<(), IpcError> {
    if token.is_empty() {
        return Err(IpcError::new(IpcError::HANDSHAKE, "empty token"));
    }
    let (mut stream, _) = accept_with_deadline(&listener, accept_timeout)?;
    stream.set_nonblocking(false).map_err(IpcError::from)?;
    stream.set_read_timeout(Some(wire::IO_TIMEOUT)).map_err(IpcError::from)?;
    stream.set_write_timeout(Some(wire::IO_TIMEOUT)).map_err(IpcError::from)?;

    let handshake_frame = wire::read_frame(&mut stream)?;
    let handshake: Handshake = serde_json::from_slice(&handshake_frame)?;
    if !token_matches(&token, &handshake.token) {
        let ack = HandshakeAck { ok: false, code: Some(IpcError::HANDSHAKE.to_string()) };
        wire::write_frame(&mut stream, &serde_json::to_vec(&ack)?)?;
        return Err(IpcError::new(IpcError::HANDSHAKE, "token mismatch"));
    }
    wire::write_frame(
        &mut stream,
        &serde_json::to_vec(&HandshakeAck { ok: true, code: None })?,
    )?;

    for _ in 0..wire::MAX_REQUESTS {
        let Some(frame) = wire::read_frame_opt(&mut stream)? else {
            return Ok(());
        };
        let request: Request = serde_json::from_slice(&frame)?;
        let response = match wire::parse_capability(&request.capability) {
            None => Response {
                id: request.id,
                ok: false,
                data: None,
                code: Some(IpcError::UNKNOWN_CAP.to_string()),
                detail: Some(request.capability),
            },
            Some(area) => match handler.read(area, request.arg.as_deref()) {
                Ok(data) if data.len().saturating_add(1024) > wire::MAX_FRAME_BYTES => Response {
                    id: request.id,
                    ok: false,
                    data: None,
                    code: Some(IpcError::RESPONSE_TOO_LARGE.to_string()),
                    detail: Some(format!("{} bytes", data.len())),
                },
                Ok(data) => Response {
                    id: request.id,
                    ok: true,
                    data: Some(data),
                    code: None,
                    detail: None,
                },
                Err(error) => Response {
                    id: request.id,
                    ok: false,
                    data: None,
                    code: Some(error.code),
                    detail: Some(error.detail),
                },
            },
        };
        wire::write_frame(&mut stream, &serde_json::to_vec(&response)?)?;
    }
    Err(IpcError::new(IpcError::TOO_MANY, "request limit reached"))
}

/// 常驻宿主用：**同一个监听器反复接新连接**（一条连接最多
/// [`wire::MAX_REQUESTS`] 个请求）。宿主撞上限后重连，服务端继续服务。
///
/// `stop` 由启动器在宿主退出后置真；每次 `accept` 最长等 `accept_timeout`，
/// 所以退出最多慢一个短超时，不会把启动器挂住。
pub fn serve_repeated<H: ReadHandler>(
    listener: TcpListener,
    token: String,
    handler: &mut H,
    stop: Arc<AtomicBool>,
    accept_timeout: Duration,
) -> Result<(), IpcError> {
    while !stop.load(Ordering::Relaxed) {
        let connection = listener.try_clone().map_err(IpcError::from)?;
        match serve_one_with_timeout(connection, token.clone(), handler, accept_timeout) {
            Ok(()) => {}
            // 单条连接的错（超时 / 撞上限 / 握手失败 / 被重置）都不该
            // 把监听器带走：常驻宿主下一轮还要重连。
            Err(_) => {}
        }
    }
    Ok(())
}

/// 多模块服务端：**同一监听器按请求里的 `module_id` 路由到不同 handler**。
///
/// 连接可反复重连；单条连接仍受 [`wire::MAX_REQUESTS`] 限制。单模块的
/// `module_id=None` 只在 map 恰好一个 handler 时放行（兼容老调用）。
pub fn serve_multi<H: ReadHandler + Send + 'static>(
    listener: TcpListener,
    token: String,
    handlers: Arc<std::sync::Mutex<HashMap<String, H>>>,
    stop: Arc<AtomicBool>,
    accept_timeout: Duration,
) -> Result<(), IpcError> {
    if token.is_empty() {
        return Err(IpcError::new(IpcError::HANDSHAKE, "empty token"));
    }
    while !stop.load(Ordering::Relaxed) {
        let connection = listener.try_clone().map_err(IpcError::from)?;
        let (stream, _) = match accept_with_deadline(&connection, accept_timeout) {
            Ok(pair) => pair,
            Err(error) if error.code == IpcError::TIMEOUT => continue,
            Err(error) => return Err(error),
        };
        let token = token.clone();
        let handlers = handlers.clone();
        std::thread::spawn(move || {
            let _ = serve_multi_connection(stream, token, handlers);
        });
    }
    Ok(())
}

fn serve_multi_connection<H: ReadHandler + Send + 'static>(
    mut stream: TcpStream,
    token: String,
    handlers: Arc<std::sync::Mutex<HashMap<String, H>>>,
) -> Result<(), IpcError> {
    stream.set_nonblocking(false).map_err(IpcError::from)?;
    stream.set_read_timeout(Some(wire::IO_TIMEOUT)).map_err(IpcError::from)?;
    stream.set_write_timeout(Some(wire::IO_TIMEOUT)).map_err(IpcError::from)?;
    let handshake: Handshake = serde_json::from_slice(&wire::read_frame(&mut stream)?)?;
    if !token_matches(&token, &handshake.token) {
        let ack = HandshakeAck { ok: false, code: Some(IpcError::HANDSHAKE.to_string()) };
        let _ = wire::write_frame(&mut stream, &serde_json::to_vec(&ack)?);
        return Ok(());
    }
    wire::write_frame(
        &mut stream,
        &serde_json::to_vec(&HandshakeAck { ok: true, code: None })?,
    )?;

    for _ in 0..wire::MAX_REQUESTS {
        let Some(frame) = wire::read_frame_opt(&mut stream)? else {
            return Ok(());
        };
        let request: Request = serde_json::from_slice(&frame)?;
        let mut map = handlers
            .lock()
            .map_err(|_| IpcError::new(IpcError::IO, "handler map poisoned"))?;
        let target = match request.module_id.as_deref() {
            Some(module_id) => map.get_mut(module_id),
            None if map.len() == 1 => map.values_mut().next(),
            None => None,
        };
        let response = match target {
            None => Response {
                id: request.id,
                ok: false,
                data: None,
                code: Some(IpcError::UNKNOWN_MODULE.to_string()),
                detail: request.module_id,
            },
            Some(handler) => match wire::parse_capability(&request.capability) {
                None => Response {
                    id: request.id,
                    ok: false,
                    data: None,
                    code: Some(IpcError::UNKNOWN_CAP.to_string()),
                    detail: Some(request.capability),
                },
                Some(area) => match handler.read(area, request.arg.as_deref()) {
                    Ok(data) if data.len().saturating_add(1024) > wire::MAX_FRAME_BYTES => {
                        Response {
                            id: request.id,
                            ok: false,
                            data: None,
                            code: Some(IpcError::RESPONSE_TOO_LARGE.to_string()),
                            detail: Some(format!("{} bytes", data.len())),
                        }
                    }
                    Ok(data) => Response {
                        id: request.id,
                        ok: true,
                        data: Some(data),
                        code: None,
                        detail: None,
                    },
                    Err(error) => Response {
                        id: request.id,
                        ok: false,
                        data: None,
                        code: Some(error.code),
                        detail: Some(error.detail),
                    },
                },
            },
        };
        wire::write_frame(&mut stream, &serde_json::to_vec(&response)?)?;
    }
    Ok(())
}

/// 宿主侧：连回环、握手、按能力读。
#[derive(Debug)]
pub struct Client {
    stream: TcpStream,
    next_id: u64,
    module_id: Option<String>,
}

impl Client {
    /// 连上启动器的 IPC；**非回环地址直接拒**（token 不出本机）。
    pub fn connect(addr: SocketAddr, token: &str) -> Result<Self, IpcError> {
        Self::connect_module(addr, token, None)
    }

    /// 多模块宿主：连接时绑定模块 id，之后每个请求都带它。
    pub fn connect_module(
        addr: SocketAddr,
        token: &str,
        module_id: Option<&str>,
    ) -> Result<Self, IpcError> {
        if !addr.ip().is_loopback() {
            return Err(IpcError::new(IpcError::NOT_LOOPBACK, addr.to_string()));
        }
        let mut stream =
            TcpStream::connect_timeout(&addr, wire::IO_TIMEOUT).map_err(IpcError::from)?;
        stream.set_read_timeout(Some(wire::IO_TIMEOUT)).map_err(IpcError::from)?;
        stream.set_write_timeout(Some(wire::IO_TIMEOUT)).map_err(IpcError::from)?;
        wire::write_frame(
            &mut stream,
            &serde_json::to_vec(&Handshake { token: token.to_string() })?,
        )?;
        let ack: HandshakeAck = serde_json::from_slice(&wire::read_frame(&mut stream)?)?;
        if !ack.ok {
            return Err(IpcError::new(
                IpcError::HANDSHAKE,
                ack.code.unwrap_or_else(|| "rejected".to_string()),
            ));
        }
        Ok(Self {
            stream,
            next_id: 1,
            module_id: module_id.map(str::to_string),
        })
    }

    /// 按区域读；`arg` 是整数 id 的十进制文本（`works` 不需要）。
    pub fn read(
        &mut self,
        area: yanmo_proto::capability::CoreDataArea,
        arg: Option<&str>,
    ) -> Result<String, IpcError> {
        self.request(area.as_str(), arg)
    }

    /// 低层入口（测试未知能力 / 将来扩展用）；**服务端仍按区域白名单拒**。
    pub fn request(&mut self, capability: &str, arg: Option<&str>) -> Result<String, IpcError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = Request {
            id,
            capability: capability.to_string(),
            arg: arg.map(str::to_string),
            module_id: self.module_id.clone(),
        };
        wire::write_frame(&mut self.stream, &serde_json::to_vec(&request)?)?;
        let response: Response = serde_json::from_slice(&wire::read_frame(&mut self.stream)?)?;
        if response.id != id {
            return Err(IpcError::new(
                IpcError::PROTOCOL,
                format!("response id {} for request {id}", response.id),
            ));
        }
        if response.ok {
            Ok(response.data.unwrap_or_default())
        } else {
            Err(IpcError::new(
                response.code.unwrap_or_else(|| "ipc.denied".to_string()),
                response.detail.unwrap_or_default(),
            ))
        }
    }
}

fn accept_with_deadline(
    listener: &TcpListener,
    timeout: Duration,
) -> Result<(TcpStream, SocketAddr), IpcError> {
    listener.set_nonblocking(true).map_err(IpcError::from)?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok(connection) => return Ok(connection),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(IpcError::new(IpcError::TIMEOUT, "no host connected"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(IpcError::from(error)),
        }
    }
}

fn token_matches(expected: &str, actual: &str) -> bool {    let expected = expected.as_bytes();
    let actual = actual.as_bytes();
    if expected.len() != actual.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in expected.iter().zip(actual) {
        difference |= left ^ right;
    }
    difference == 0
}
