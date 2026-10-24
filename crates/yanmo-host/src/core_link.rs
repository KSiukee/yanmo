//! 核心连接：**断了能重连**。
//!
//! 常驻宿主一条连接最多用 [`yanmo_proto::ipc::MAX_REQUESTS`] 个请求；撞上限后
//! 服务端丢掉这条连接，下一次 `read` 用同一 token 重新握手。请求是幂等读，
//! 重连后重试一次是安全的。

use std::net::SocketAddr;

use yanmo_ipc::Client;
use yanmo_proto::capability::CoreDataArea;
use yanmo_proto::ipc::IpcError;

pub struct CoreLink {
    endpoint: Option<(SocketAddr, String)>,
    client: Option<Client>,
}

impl CoreLink {
    pub fn new(addr: SocketAddr, token: String) -> Self {
        Self {
            endpoint: Some((addr, token)),
            client: None,
        }
    }

    /// 已有连接、但没有重连端点（一次性宿主 / 测试）。
    pub fn from_client(client: Client) -> Self {
        Self {
            endpoint: None,
            client: Some(client),
        }
    }

    pub fn read(&mut self, area: CoreDataArea, arg: Option<&str>) -> Result<String, IpcError> {
        if self.client.is_none() {
            self.connect()?;
        }
        let first = self
            .client
            .as_mut()
            .ok_or_else(|| IpcError::new(IpcError::IO, "core link is not connected"))?
            .read(area, arg);
        let Err(error) = &first else {
            return first;
        };
        let retryable = matches!(
            error.code.as_str(),
            IpcError::IO | IpcError::TIMEOUT | IpcError::PROTOCOL | IpcError::HANDSHAKE
                | IpcError::TOO_MANY
        );
        if !retryable || self.endpoint.is_none() {
            return first;
        }
        self.client = None;
        let mut last = None;
        for attempt in 0..3 {
            match self.connect() {
                Ok(()) => {
                    last = None;
                    break;
                }
                Err(error) => {
                    last = Some(error);
                    if attempt < 2 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
        }
        if let Some(error) = last {
            return Err(error);
        }
        self.client
            .as_mut()
            .ok_or_else(|| IpcError::new(IpcError::IO, "core link reconnect failed"))?
            .read(area, arg)
    }

    fn connect(&mut self) -> Result<(), IpcError> {
        let Some((addr, token)) = self.endpoint.clone() else {
            return Err(IpcError::new(IpcError::IO, "no reconnect endpoint"));
        };
        self.client = Some(Client::connect(addr, &token)?);
        Ok(())
    }
}
