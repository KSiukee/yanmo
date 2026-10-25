//! 模块凭据绑定：**只存元数据，不存秘密**。
//!
//! 秘密在系统凭据管理器（`yanmo-vault`）；这里只记哪个模块能用哪个名字、
//! 只允许把 `Authorization` 注入到哪个域名。这样核心库被拷走也拿不到 key。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 一条凭据绑定（名字 + 允许的域名）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialBinding {
    pub name: String,
    pub host: String,
}

const MAX_NAME: usize = 64;
const MAX_HOST: usize = 253;

impl super::Store {
    /// 列出某个模块的凭据绑定；**不含秘密**。
    pub fn module_credentials(&self, module_id: &str) -> Result<Vec<CredentialBinding>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, host FROM module_credentials WHERE module_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![module_id], |row| {
            Ok(CredentialBinding { name: row.get(0)?, host: row.get(1)? })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 新增或更新一条绑定；同一模块同名只保留一条。
    pub fn add_module_credential(
        &mut self,
        module_id: &str,
        name: &str,
        host: &str,
    ) -> Result<()> {
        self.module_record(module_id)?;
        validate_name(name)?;
        validate_host(host)?;
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO module_credentials(module_id, name, host, created_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(module_id, name) DO UPDATE SET host = excluded.host",
            params![module_id, name, host, now],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "module_credentials",
            0,
            "add_credential",
            serde_json::json!({ "module_id": module_id, "name": name, "host": host }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 删一条绑定；不存在不算错。
    pub fn remove_module_credential(&mut self, module_id: &str, name: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "DELETE FROM module_credentials WHERE module_id = ?1 AND name = ?2",
            params![module_id, name],
        )?;
        if changed > 0 {
            Self::record_in(
                &self.device_id,
                &tx,
                "module_credentials",
                0,
                "remove_credential",
                serde_json::json!({ "module_id": module_id, "name": name }),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 某模块是否绑定了这个名字（宿主代理只认绑定过的名字）。
    pub fn has_module_credential(&self, module_id: &str, name: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM module_credentials WHERE module_id = ?1 AND name = ?2",
                params![module_id, name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > MAX_NAME
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Error::invalid_with(
            codes::MODULE_CREDENTIAL_INVALID,
            [("field", "name".to_string())],
        ));
    }
    Ok(())
}

fn validate_host(host: &str) -> Result<()> {
    if host.is_empty()
        || host.len() > MAX_HOST
        || host.contains("://")
        || host.contains('/')
        || host.contains(' ')
        || host.contains(':')
    {
        return Err(Error::invalid_with(
            codes::MODULE_CREDENTIAL_INVALID,
            [("field", "host".to_string())],
        ));
    }
    let bare = host.strip_prefix("*.").unwrap_or(host);
    if bare.is_empty() || bare.contains('*') || !bare.contains('.') {
        return Err(Error::invalid_with(
            codes::MODULE_CREDENTIAL_INVALID,
            [("field", "host".to_string())],
        ));
    }
    Ok(())
}
