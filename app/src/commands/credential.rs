//! 模块凭据：**秘密只进系统钥匙串，核心只存绑定元数据**。
//!
//! 界面永远只能拿到名字与域名；`secret` 参数只在这条命令里走一次，不落日志、不回读。

use tauri::State;

use super::module_run::ModuleRuns;
use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::store::{CredentialBinding, Store};
use yanmo_vault::{self as vault, VaultError};

#[tauri::command(rename_all = "snake_case")]
pub fn credentials_list(
    data: State<'_, AppData>,
    module_id: String,
) -> Result<Vec<CredentialBinding>, ApiError> {
    data.with_store(|store: &mut Store| store.module_credentials(&module_id))
}

#[tauri::command(rename_all = "snake_case")]
pub fn credential_add(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
    name: String,
    host: String,
    secret: String,
) -> Result<CredentialBinding, ApiError> {
    // 先确认模块存在，再动钥匙串；任何一步失败都不留半条绑定。
    data.with_store(|store: &mut Store| store.module_record(&module_id))?;
    vault::validate_name(&name).map_err(vault_error)?;
    vault::validate_host(&host).map_err(vault_error)?;
    vault::store(&module_id, &name, &secret).map_err(vault_error)?;
    let binding = CredentialBinding { name: name.clone(), host: host.clone() };
    if let Err(error) = data.with_store(|store: &mut Store| {
        store.add_module_credential(&module_id, &name, &host)
    }) {
        let _ = vault::remove(&module_id, &name);
        return Err(error.into());
    }
    runs.stop_platform()?;
    Ok(binding)
}

#[tauri::command(rename_all = "snake_case")]
pub fn credential_remove(
    data: State<'_, AppData>,
    runs: State<'_, ModuleRuns>,
    module_id: String,
    name: String,
) -> Result<(), ApiError> {
    vault::remove(&module_id, &name).map_err(vault_error)?;
    data.with_store(|store: &mut Store| store.remove_module_credential(&module_id, &name))?;
    runs.stop_platform()?;
    Ok(())
}

fn vault_error(error: VaultError) -> ApiError {
    ApiError::new("shell.credential_failed")
        .caused_by(format!("{}: {}", error.code, error.detail))
}
