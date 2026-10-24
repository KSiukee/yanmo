//! IPC 验收：**回环、一次性 token、按区域白名单、有界帧**。
//!
//! 这组只测协议与传输（不碰数据库）；核心侧的真实读取在 `yanmo-core` 的
//! `module_ipc` 测试里接真库验。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use yanmo_ipc::{bind_loopback, serve_one, serve_one_with_timeout, serve_repeated, Client};
use yanmo_proto::capability::CoreDataArea;
use yanmo_proto::ipc::{self, IpcError, ReadHandler};

struct Mock;

impl ReadHandler for Mock {
    fn read(&mut self, area: CoreDataArea, arg: Option<&str>) -> Result<String, IpcError> {
        match area {
            CoreDataArea::Works => Ok(r#"{"works":[]}"#.to_string()),
            CoreDataArea::Tree => Ok(format!(r#"{{"work":{}}}"#, arg.unwrap_or("0"))),
            CoreDataArea::Body => Err(IpcError::new("module.capability_denied", "body")),
            _ => Err(IpcError::new("ipc.capability_unavailable", area.as_str())),
        }
    }
}

fn start(token: &str) -> (SocketAddr, JoinHandle<Result<(), IpcError>>) {
    let listener = bind_loopback().unwrap();
    let addr = listener.local_addr().unwrap();
    let token = token.to_string();
    let handle = std::thread::spawn(move || {
        let mut handler = Mock;
        serve_one(listener, token, &mut handler)
    });
    (addr, handle)
}

#[test]
fn a_client_reads_only_the_capabilities_the_core_answers() {
    let (addr, handle) = start("secret");
    let mut client = Client::connect(addr, "secret").unwrap();
    assert_eq!(client.read(CoreDataArea::Works, None).unwrap(), r#"{"works":[]}"#);
    assert_eq!(
        client.read(CoreDataArea::Tree, Some("7")).unwrap(),
        r#"{"work":7}"#
    );

    let denied = client.read(CoreDataArea::Body, Some("1")).unwrap_err();
    assert_eq!(denied.code, "module.capability_denied");

    let unknown = client.request("nope", None).unwrap_err();
    assert_eq!(unknown.code, IpcError::UNKNOWN_CAP);

    drop(client);
    handle.join().unwrap().unwrap();
}

#[test]
fn a_wrong_token_is_rejected_before_any_read() {
    let (addr, handle) = start("secret");
    let error = Client::connect(addr, "wrong").unwrap_err();
    assert_eq!(error.code, IpcError::HANDSHAKE);
    assert!(handle.join().unwrap().is_err(), "服务端也要报错，不是静默放行");
}

#[test]
fn the_listener_is_gone_after_one_host() {
    let (addr, handle) = start("secret");
    {
        let mut client = Client::connect(addr, "secret").unwrap();
        client.read(CoreDataArea::Works, None).unwrap();
    }
    handle.join().unwrap().unwrap();
    // 监听器已经随 serve_one 一起销毁：同一个 token 开不了第二条连接
    assert!(
        std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err(),
        "一次性 token 不能复用"
    );
}

#[test]
fn frames_are_bounded() {
    let mut sink = Vec::new();
    let empty = ipc::write_frame(&mut sink, &[]).unwrap_err();
    assert_eq!(empty.code, IpcError::FRAME);

    let mut oversized = vec![0u8; ipc::MAX_FRAME_BYTES + 1];
    let too_big = ipc::write_frame(&mut sink, &oversized).unwrap_err();
    assert_eq!(too_big.code, IpcError::FRAME);

    oversized.clear();
    oversized.extend_from_slice(&((ipc::MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
    let refused = ipc::read_frame(&mut oversized.as_slice()).unwrap_err();
    assert_eq!(refused.code, IpcError::FRAME);
}

#[test]
fn non_loopback_addresses_are_refused_without_connecting() {
    let error = Client::connect("192.0.2.1:9".parse().unwrap(), "token").unwrap_err();
    assert_eq!(error.code, IpcError::NOT_LOOPBACK);
}

#[test]
fn a_server_with_no_host_times_out_instead_of_hanging() {
    let listener = bind_loopback().unwrap();
    let mut handler = Mock;
    let error = serve_one_with_timeout(
        listener,
        "secret".to_string(),
        &mut handler,
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert_eq!(error.code, IpcError::TIMEOUT);
}

#[test]
fn capability_codes_round_trip() {
    for area in CoreDataArea::ALL {
        assert_eq!(ipc::parse_capability(area.as_str()), Some(area));
    }
    assert_eq!(ipc::parse_capability("sql"), None);
}

#[test]
fn a_repeated_server_accepts_a_second_connection() {
    let listener = bind_loopback().unwrap();
    let addr = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let handle = std::thread::spawn(move || {
        let mut handler = Mock;
        serve_repeated(
            listener,
            "secret".to_string(),
            &mut handler,
            server_stop,
            Duration::from_millis(200),
        )
    });

    {
        let mut first = Client::connect(addr, "secret").unwrap();
        assert_eq!(first.read(CoreDataArea::Works, None).unwrap(), r#"{"works":[]}"#);
    }
    {
        let mut second = Client::connect(addr, "secret").unwrap();
        assert_eq!(second.read(CoreDataArea::Works, None).unwrap(), r#"{"works":[]}"#);
    }
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap().unwrap();
}
