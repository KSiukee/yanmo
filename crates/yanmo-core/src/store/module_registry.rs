//! 模块注册表：**装了谁、声明了什么、用户给了什么、冻结没有、认证没有**。
//!
//! 这本账在 `yanmo.db`（迁移 v22），不在 `modules.db`——理由见迁移文件头。
//! 模块自己碰不到它；只有核心的安装 / 授权 / 冻结 / 认证四个动作会写。
//!
//! # 三条硬口径
//!
//! 1. **授予永远是声明的子集**：不能给模块没声明过的能力（写入前校验）。
//! 2. **缺省全拒**：安装时授予什么就是什么；`{}` 就是一样都不给。
//! 3. **认证不是自述**：`certified` 只有核心验签之后才能置位（见认证那一族）；
//!    清单里没有这一格。
//!
//! # 留痕
//!
//! `op_log.entity = "modules"`、`entity_id = 0`（模块 id 是文本，放 payload 里），
//! `op` 为 `install` / `grant` / `freeze` / `enable` / `certify` / `uncertify`
//! （启动与卸载的留痕在 [`super::module_lifecycle`]）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::manifest::ModuleManifest;
use yanmo_proto::ManifestError;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{first_ungrantable, ModuleRecord};
use crate::time::now_millis;

/// 读记录时的列顺序（与 [`decode`] 的参数顺序一一对应）。
const COLS: &str = "manifest, granted, enabled, certified, installed_at, updated_at";

fn map_manifest_error(error: ManifestError) -> Error {
    Error::invalid_with(
        codes::MODULE_MANIFEST_INVALID,
        [("code", error.code.to_string()), ("field", error.field.to_string())],
    )
}

fn map_grant_error(error: ManifestError) -> Error {
    Error::invalid_with(
        codes::MODULE_GRANT_INVALID,
        [("detail", format!("{} at {}", error.code, error.field))],
    )
}

fn not_found(module_id: &str) -> Error {
    Error::invalid_with(codes::MODULE_NOT_FOUND, [("module_id", module_id.to_string())])
}

/// 把一行记录解成 [`ModuleRecord`]。
///
/// 清单坏了是**记录损坏**（报错，让界面能看见）；授予坏了**当作全拒**
/// （fail closed，宁可少给，不能多给）。
fn decode(
    module_id: String,
    manifest_text: String,
    granted_text: String,
    enabled: bool,
    certified: bool,
    installed_at: i64,
    updated_at: i64,
) -> Result<ModuleRecord> {
    let manifest = ModuleManifest::from_json(&manifest_text).map_err(|_| {
        Error::invalid_with(codes::MODULE_RECORD_CORRUPT, [("module_id", module_id.clone())])
    })?;
    let granted: ModuleCapabilities = serde_json::from_str(&granted_text).unwrap_or_default();
    Ok(ModuleRecord { manifest, granted, enabled, certified, installed_at, updated_at })
}

fn encode_grant(granted: &ModuleCapabilities) -> String {
    serde_json::to_string(granted).unwrap_or_else(|_| "{}".to_string())
}

impl Store {
    /// 安装一个模块：校验清单与授予，落注册表，**不碰模块文件**（导入闸在另一条线）。
    ///
    /// 重复安装同一个 id 会被拒（升级走更新纪律，不在这里悄悄覆盖）。
    pub fn install_module(
        &mut self,
        manifest: &ModuleManifest,
        granted: &ModuleCapabilities,
    ) -> Result<()> {
        self.insert_module(manifest, granted, "", "", "install", None)
    }

    /// 落注册表的唯一入口（导入闸会带载荷指纹、来源与认证）。
    pub(crate) fn insert_module(
        &mut self,
        manifest: &ModuleManifest,
        granted: &ModuleCapabilities,
        payload_sha256: &str,
        source: &str,
        op: &str,
        certification: Option<&super::certification::Certification>,
    ) -> Result<()> {
        manifest.validate().map_err(map_manifest_error)?;
        granted.validate(&manifest.id).map_err(map_grant_error)?;
        if let Some(capability) = first_ungrantable(&manifest.capabilities, granted) {
            return Err(Error::invalid_with(
                codes::MODULE_GRANT_NOT_DECLARED,
                [("capability", capability)],
            ));
        }

        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM modules WHERE id = ?1)",
            [manifest.id.as_str()],
            |row| row.get(0),
        )?;
        if exists {
            return Err(Error::invalid_with(
                codes::MODULE_ALREADY_INSTALLED,
                [("module_id", manifest.id.clone())],
            ));
        }

        let manifest_text = serde_json::to_string(manifest).map_err(|_| {
            Error::invalid_with(
                codes::MODULE_MANIFEST_INVALID,
                [("code", ManifestError::JSON.to_string()), ("field", "manifest".to_string())],
            )
        })?;
        let granted_text = encode_grant(granted);
        let now = now_millis();
        let certified = i64::from(certification.is_some());
        let cert_edition = certification.map(|cert| cert.edition).unwrap_or(0) as i64;
        let certified_at = certification.map(|cert| cert.certified_at).unwrap_or(0);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO modules(id, manifest, version, protocol, certified, cert_edition,
                                 certified_at, enabled, granted, payload_sha256, source,
                                 installed_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10, ?11, ?11)",
            params![
                manifest.id,
                manifest_text,
                manifest.version,
                manifest.protocol,
                certified,
                cert_edition,
                certified_at,
                granted_text,
                payload_sha256,
                source,
                now
            ],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            op,
            json!({
                "module_id": manifest.id,
                "version": manifest.version,
                "protocol": manifest.protocol,
                "granted": granted,
                "payload_sha256": payload_sha256,
                "source": source,
                "certified": certification.is_some(),
                "cert_edition": cert_edition,
                "certified_at": certified_at,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 一条模块记录；没装过 → `module.not_found`。
    pub fn module_record(&self, module_id: &str) -> Result<ModuleRecord> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM modules WHERE id = ?1"),
                [module_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Err(not_found(module_id)),
            Some((manifest, granted, enabled, certified, installed_at, updated_at)) => decode(
                module_id.to_string(),
                manifest,
                granted,
                enabled,
                certified,
                installed_at,
                updated_at,
            ),
        }
    }

    /// 全部模块记录（按安装时间，早装的在前）。
    pub fn module_records(&self) -> Result<Vec<ModuleRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, {COLS} FROM modules ORDER BY installed_at, id"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, manifest, granted, enabled, certified, installed_at, updated_at) = row?;
            out.push(decode(id, manifest, granted, enabled, certified, installed_at, updated_at)?);
        }
        Ok(out)
    }

    /// 重新授予能力（**只能给声明过的**；给什么就是什么，不是增量）。
    pub fn grant_module(
        &mut self,
        module_id: &str,
        granted: &ModuleCapabilities,
    ) -> Result<()> {
        let record = self.module_record(module_id)?;
        granted.validate(module_id).map_err(map_grant_error)?;
        if let Some(capability) = first_ungrantable(&record.manifest.capabilities, granted) {
            return Err(Error::invalid_with(
                codes::MODULE_GRANT_NOT_DECLARED,
                [("capability", capability)],
            ));
        }
        let text = encode_grant(granted);
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE modules SET granted = ?2, updated_at = ?3 WHERE id = ?1",
            params![module_id, text, now],
        )?;
        if changed == 0 {
            return Err(not_found(module_id));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            "grant",
            json!({ "module_id": module_id, "granted": granted }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 冻结 / 解冻。冻结即不可读、不可问、不可联网；**数据与注册表都保留**。
    pub fn set_module_enabled(&mut self, module_id: &str, enabled: bool) -> Result<()> {
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE modules SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![module_id, enabled, now],
        )?;
        if changed == 0 {
            return Err(not_found(module_id));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            if enabled { "enable" } else { "freeze" },
            json!({ "module_id": module_id }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 写入**核心验签之后**的认证状态（模块自述不算数）。
    pub fn set_module_certified(&mut self, module_id: &str, certified: bool) -> Result<()> {
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE modules SET certified = ?2, updated_at = ?3 WHERE id = ?1",
            params![module_id, certified, now],
        )?;
        if changed == 0 {
            return Err(not_found(module_id));
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            if certified { "certify" } else { "uncertify" },
            json!({ "module_id": module_id }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 这个模块现在能不能读核心的这块数据（没装 → 错误；冻结 / 没授予 → `false`）。
    pub fn module_can_read(
        &self,
        module_id: &str,
        area: yanmo_proto::capability::CoreDataArea,
    ) -> Result<bool> {
        Ok(self.module_record(module_id)?.can_read(area))
    }

}
