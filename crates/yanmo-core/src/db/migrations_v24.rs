//! 迁移 v24：**导入闸的账**——载荷指纹与来源。
//!
//! ⚠️ 与 v1~v23 同样**一旦发布不可再改**。
//!
//! `payload_sha256` 是导入时算的入口 `.wasm` 的 SHA-256；认证状态将来绑定它
//! （认证说的是"这份代码是官方发的"，不是"这个人付了钱"）。
//! `source` 是**短标签**（如 `folder`），不是完整本机路径——界面不显示路径。
//!
//! 老库里的模块没有这两列的值：默认空串，读作"没记录"。

use rusqlite::Connection;

use crate::error::Result;

/// v24 的静态步骤：本版没有（结构变更全在 [`prepare`] 里按需产出）。
pub const STEPS: &[&str] = &[];

pub fn prepare(conn: &Connection) -> Result<Vec<String>> {
    let mut steps = Vec::new();
    if !column_exists(conn, "modules", "payload_sha256")? {
        steps.push(
            "ALTER TABLE modules ADD COLUMN payload_sha256 TEXT NOT NULL DEFAULT ''".to_string(),
        );
    }
    if !column_exists(conn, "modules", "source")? {
        steps.push("ALTER TABLE modules ADD COLUMN source TEXT NOT NULL DEFAULT ''".to_string());
    }
    Ok(steps)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // 表名是常量字面量，不是外部输入——PRAGMA 不支持参数占位，只能拼
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
