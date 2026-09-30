//! 加密备份包的命令面：开关、口令、恢复码。
//!
//! 三条边界（与别处一致）：
//!
//! - **口令只在内存**（[`crate::seal_gate`]）：这里只负责"记住 / 撤掉"，绝不写库写盘；
//! - **恢复码只在开启时给一次**（核心 `generate_recovery_code` + `set_seal_recovery`）：
//!   已经有一把就不再往外吐——作者要的是"抄下来"，不是"随时能再看一眼"；
//! - **这一族只管钥匙与开关**，备份怎么封、恢复怎么开在各自那两条命令里（backup / restore 域）。

use serde::Serialize;
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::Store;

/// 加密那一格的现状（设置页那一组要的三件事）。
#[derive(Debug, Serialize)]
pub struct SealStatusDto {
    /// 作者要不要加密（偏好里那一格）
    pub enabled: bool,
    /// 此刻手里有没有口令——**重启之后会是 false**，界面要如实说
    pub unlocked: bool,
    /// 本机有没有记着一把恢复码
    pub has_recovery: bool,
}

/// 读加密那一格的现状。**不含任何钥匙本身**。
#[tauri::command(rename_all = "snake_case")]
pub fn seal_status(data: State<'_, AppData>) -> Result<SealStatusDto, ApiError> {
    let enabled = data.with_store(|store: &mut Store| Ok(store.backup_config()?.seal))?;
    let has_recovery = data.with_store(|store: &mut Store| Ok(store.seal_recovery()?.is_some()))?;
    Ok(SealStatusDto { enabled, unlocked: crate::seal_gate::unlocked(), has_recovery })
}

/// 开启加密：记下口令（内存）→ 打开开关（偏好）→ 需要的话生成一把恢复码并**当场给它一次**。
///
/// 返回 `Some(恢复码)` 只在"本机还没有过恢复码"时发生——界面那时必须把码给作者抄下来；
/// 已经有一把就返回 `None`（换掉它会让以前那些包失去第二把钥匙）。
#[tauri::command(rename_all = "snake_case")]
pub fn seal_enable(data: State<'_, AppData>, passphrase: String) -> Result<Option<String>, ApiError> {
    if passphrase.trim().is_empty() {
        return Err(ApiError::new("backup.seal_passphrase_empty"));
    }
    crate::seal_gate::set(&passphrase);
    // 开关与恢复码在同一个 with_store 里落定：两者是一件事（"以后写出去的包要加密"）
    data.with_store(|store: &mut Store| {
        let mut config = store.backup_config()?;
        config.seal = true;
        store.set_backup_config(&config)?;
        if store.seal_recovery()?.is_some() {
            return Ok(None);
        }
        let code = yanmo_core::seal::generate_recovery_code()?;
        store.set_seal_recovery(&code)?;
        Ok(Some(code))
    })
}

/// 关掉加密：**只关开关**，恢复码与本机记着的那把钥匙都不动
/// （以前写出去的包还要靠它们开；作者纸上的那一串也是它）。
#[tauri::command(rename_all = "snake_case")]
pub fn seal_disable(data: State<'_, AppData>) -> Result<SealStatusDto, ApiError> {
    crate::seal_gate::clear();
    let (enabled, has_recovery) = data.with_store(|store: &mut Store| {
        let mut config = store.backup_config()?;
        config.seal = false;
        store.set_backup_config(&config)?;
        Ok((config.seal, store.seal_recovery()?.is_some()))
    })?;
    Ok(SealStatusDto { enabled, unlocked: false, has_recovery })
}

/// 给口令（内存）：重启之后、或者这次要拿恢复码去开一个包时用。
///
/// **这里验不了对错**——口令对不对，只有真的去解一个包才知道（那时报「打不开」）。
#[tauri::command(rename_all = "snake_case")]
pub fn seal_unlock(passphrase: String) -> Result<SealStatusDto, ApiError> {
    if passphrase.trim().is_empty() {
        return Err(ApiError::new("backup.seal_passphrase_empty"));
    }
    crate::seal_gate::set(&passphrase);
    Ok(SealStatusDto { enabled: true, unlocked: true, has_recovery: true })
}

/// 把口令从内存里撤掉（"锁上"）。**不动开关**：作者仍要加密，只是这一会儿没给口令。
#[tauri::command]
pub fn seal_lock() {
    crate::seal_gate::clear();
}
