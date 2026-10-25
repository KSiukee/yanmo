//! 监督者的进程小工具：回收子宿主、按同一份策略再拉一个。
//!
//! 单独成件：main 只留控制循环，重启/参数拼装不跟循环搅在一起。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub fn reap(children: &mut HashMap<String, Child>) -> Vec<(String, i32)> {
    let mut done = Vec::new();
    for (module_id, child) in children.iter_mut() {
        if let Ok(Some(status)) = child.try_wait() {
            done.push((module_id.clone(), status.code().unwrap_or(-1)));
        }
    }
    for (module_id, _) in &done {
        children.remove(module_id);
    }
    done
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_child(
    exe: &Path,
    root: &Path,
    module_id: &str,
    core_addr: Option<SocketAddr>,
    token: Option<&str>,
    network_modules: &[String],
    credentials_json: Option<&str>,
    network_log: Option<&Path>,
    network_allowlist: &[String],
) -> std::io::Result<Child> {
    let mut command = Command::new(exe);
    command
        .arg("--module")
        .arg(root.join(module_id))
        .arg("--module-id")
        .arg(module_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let has_core = core_addr.is_some() && token.is_some();
    if let (Some(addr), Some(_)) = (core_addr, token) {
        command
            .arg("--core")
            .arg(addr.to_string())
            .arg("--token-stdin");
    }
    if network_modules.iter().any(|id| id == module_id) {
        command.arg("--allow-network");
    }
    if let Some(credentials) = credentials_json {
        command.arg("--credentials-json").arg(credentials);
    }
    if let Some(log) = network_log {
        command.arg("--network-log").arg(log);
    }
    if !network_allowlist.is_empty() {
        command
            .arg("--network-allowlist")
            .arg(network_allowlist.join(","));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        if has_core {
            if let Some(token) = token {
                let _ = std::io::Write::write_all(&mut stdin, format!("{token}\n").as_bytes());
            }
        }
    }
    Ok(child)
}
