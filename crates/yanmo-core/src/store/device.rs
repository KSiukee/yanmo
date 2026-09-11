//! 本机设备标识：op-log 要能区分"哪台机器改的"。
//!
//! 生成一次、存进 `settings` 并登记到 `devices`，之后一直复用。
//!
//! ⚠️ **它不是安全凭证**——只是一个本机名字。别拿它做鉴权，也别把它当成防伪依据。

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::time::now_millis;

/// 设备标识在 `settings` 里的键。
const KEY: &str = "device_id";

/// 取本机设备标识；没有就生成一个并落库。
pub fn ensure(conn: &Connection) -> Result<String> {
    if let Some(id) = read(conn)? {
        return Ok(id);
    }
    let id = generate();
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
        params![KEY, id, now_millis()],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO devices(id, name, last_seq, created_at) VALUES(?1, ?2, 0, ?3)",
        params![id, host_name(), now_millis()],
    )?;
    Ok(id)
}

fn read(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![KEY], |r| r.get(0))
        .optional()?)
}

/// 生成 16 位十六进制标识：机器名 + 进程号 + 时间戳经哈希得到。
///
/// 不追求密码学强度——**只要同一台机器稳定、不同机器大概率不同**就够用。
fn generate() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    host_name().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    now_millis().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "local".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_is_stable_across_calls() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_ready(dir.path().join("d.db")).unwrap();
        let first = ensure(&conn).unwrap();
        let second = ensure(&conn).unwrap();
        assert_eq!(first, second, "第二次调用必须复用同一个标识");
        assert_eq!(first.len(), 16);
        let devices: i64 = conn
            .query_row("SELECT COUNT(*) FROM devices WHERE id = ?1", params![first], |r| r.get(0))
            .unwrap();
        assert_eq!(devices, 1, "设备应登记进 devices 表");
    }
}
