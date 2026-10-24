//! 模块管理页的数据：**列表卡片 + 收藏 / 置顶偏好**（只读列表 + 两列偏好）。
//!
//! 这里不解析模块数据、不碰文件：卡片是给界面看的摘要；偏好是库级排序/收纳，
//! **不进导出、不改模块数据**。真正的启用/冻结/卸载在注册表与生命周期那两个文件里。

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use yanmo_proto::capability::ModuleCapabilities;
use yanmo_proto::localized::localized_text;
use yanmo_proto::manifest::ModuleManifest;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::time::now_millis;

/// 管理页一行（不含路径、不含授予的逐项能力参数——只有给人看的那几格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleCard {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    /// 核心验签后的认证状态（现在一律 false，认证那一刀之后才有 true）
    pub certified: bool,
    /// 已激活（未冻结）
    pub enabled: bool,
    pub favorite: bool,
    pub pinned: bool,
    /// 用户实际授予的只读区域码
    pub reads: Vec<String>,
    pub own_data: bool,
    pub network: bool,
    /// 导入时算的入口 `.wasm` SHA-256（老库 / 测试里可能是空串）
    pub fingerprint: String,
    /// 导入来源短标签（如 `folder`；不是本机路径）
    pub source: String,
    pub installed_at: i64,
    pub updated_at: i64,
}

impl Store {
    /// 管理页列表：**置顶 → 收藏 → 安装顺序**。
    pub fn module_cards(&self) -> Result<Vec<ModuleCard>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, manifest, granted, enabled, certified, favorite, pinned,
                    payload_sha256, source, installed_at, updated_at
             FROM modules
             ORDER BY pinned DESC, favorite DESC, installed_at, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;
        let mut cards = Vec::new();
        for row in rows {
            let (
                id,
                manifest_text,
                granted_text,
                enabled,
                certified,
                favorite,
                pinned,
                fingerprint,
                source,
                installed_at,
                updated_at,
            ) = row?;
            cards.push(decode(
                &id,
                &manifest_text,
                &granted_text,
                enabled,
                certified,
                favorite,
                pinned,
                &fingerprint,
                &source,
                installed_at,
                updated_at,
            )?);
        }
        Ok(cards)
    }

    /// 一条管理页卡片；没装过 → `module.not_found`。
    pub fn module_card(&self, module_id: &str) -> Result<ModuleCard> {
        self.module_cards()?
            .into_iter()
            .find(|card| card.id == module_id)
            .ok_or_else(|| not_found(module_id))
    }

    /// 收藏（不影响功能，只影响管理页排序）。
    pub fn set_module_favorite(&mut self, module_id: &str, favorite: bool) -> Result<()> {
        self.set_module_preference(module_id, Some(favorite), None)
    }

    /// 置顶（不影响功能，只影响管理页排序）。
    pub fn set_module_pinned(&mut self, module_id: &str, pinned: bool) -> Result<()> {
        self.set_module_preference(module_id, None, Some(pinned))
    }

    fn set_module_preference(
        &mut self,
        module_id: &str,
        favorite: Option<bool>,
        pinned: Option<bool>,
    ) -> Result<()> {
        let current: Option<(bool, bool)> = self
            .conn
            .query_row(
                "SELECT favorite, pinned FROM modules WHERE id = ?1",
                [module_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (current_favorite, current_pinned) = current.ok_or_else(|| not_found(module_id))?;
        let favorite = favorite.unwrap_or(current_favorite);
        let pinned = pinned.unwrap_or(current_pinned);
        let now = now_millis();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE modules SET favorite = ?2, pinned = ?3, updated_at = ?4 WHERE id = ?1",
            params![module_id, favorite, pinned, now],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            "prefer",
            serde_json::json!({
                "module_id": module_id,
                "favorite": favorite,
                "pinned": pinned,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn not_found(module_id: &str) -> Error {
    Error::invalid_with(codes::MODULE_NOT_FOUND, [("module_id", module_id.to_string())])
}

#[allow(clippy::too_many_arguments)]
fn decode(
    id: &str,
    manifest_text: &str,
    granted_text: &str,
    enabled: bool,
    certified: bool,
    favorite: bool,
    pinned: bool,
    fingerprint: &str,
    source: &str,
    installed_at: i64,
    updated_at: i64,
) -> Result<ModuleCard> {
    let manifest = ModuleManifest::from_json(manifest_text).map_err(|_| {
        Error::invalid_with(codes::MODULE_RECORD_CORRUPT, [("module_id", id.to_string())])
    })?;
    // 授予坏了**当作全拒**（fail closed）
    let granted: ModuleCapabilities = serde_json::from_str(granted_text).unwrap_or_default();
    Ok(ModuleCard {
        id: manifest.id.clone(),
        name: localized_text(&manifest.name, &manifest.default_language, "zh-Hans").to_string(),
        description: localized_text(
            &manifest.description,
            &manifest.default_language,
            "zh-Hans",
        )
        .to_string(),
        version: manifest.version.clone(),
        author: manifest.author.name.clone(),
        certified,
        enabled,
        favorite,
        pinned,
        reads: granted.read.iter().map(|area| area.as_str().to_string()).collect(),
        own_data: granted.own_data,
        network: granted.network,
        fingerprint: fingerprint.to_string(),
        source: source.to_string(),
        installed_at,
        updated_at,
    })
}
