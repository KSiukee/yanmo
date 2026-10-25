//! 迁移 v27：模块凭据绑定（**只存元数据，不存秘密**）。
//!
//! 秘密在系统凭据管理器；这张表只记：哪个模块可以用哪个名字、只允许注入到哪个域名。
//! 一旦发布不可再改。

use rusqlite::Connection;

use crate::error::Result;

pub const STEPS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS module_credentials (
        module_id  TEXT    NOT NULL,
        name       TEXT    NOT NULL,
        host       TEXT    NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY(module_id, name)
    )",
    "CREATE INDEX IF NOT EXISTS idx_module_credentials_host ON module_credentials(module_id, host)",
];

pub fn prepare(_conn: &Connection) -> Result<Vec<String>> {
    Ok(Vec::new())
}
