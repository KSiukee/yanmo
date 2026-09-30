//! 加密备份包的**口令闸**：作者输入的那句口令，只在这里活着。
//!
//! # 三条分寸
//!
//! 1. **只在内存**：口令不写库、不落盘、不进日志——应用关掉就没了（下次要用得重输）。
//!    这是它作为第一把钥匙该有的样子；恢复码那一半走的是另一条路（存在本机偏好里，
//!    见 `yanmo_core::store` 的 backup_key）。
//! 2. **重启之后会出现"开了加密但口令不在"**：那种状态下**绝不退回明文包**——
//!    备份会明确报「要先给口令」，让作者知道这一刻没有备份在跑，而不是悄悄写一份弱的包。
//! 3. **不做 zeroize**：这一层防的是「口令被写进某个文件」，不是内存取证——
//!    能读本进程内存的攻击者，早就越过了 SECURITY.md 写明要防的那条线（能读这台机器的人）。
//!
//! 与 [`crate::diagnose`] / [`crate::slowcmd_wire`] 同一种写法：进程一处的状态放全局，
//! 因为它是"这一次运行里作者有没有给过口令"，与哪个窗口、哪本书都无关。

use std::sync::{Mutex, OnceLock};

static PASSPHRASE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<String>> {
    PASSPHRASE.get_or_init(|| Mutex::new(None))
}

/// 记下口令（空串等于**没有**：核心里那把钥匙本来就不收空口令）。
pub fn set(passphrase: &str) {
    let mut guard = slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = if passphrase.trim().is_empty() { None } else { Some(passphrase.to_string()) };
}

/// 现在手里有口令没有。
pub fn unlocked() -> bool {
    slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some()
}

/// 取出口令的**一份拷贝**（只在真要封包/解包的那一刻取，取完就丢）。
pub fn take() -> Option<String> {
    slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// 把口令从内存里撤掉（作者点了「锁上」，或者关掉加密开关）。
pub fn clear() {
    let mut guard = slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}
