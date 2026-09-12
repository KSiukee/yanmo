//! 数据库层：打开、配置、环境校验。
//!
//! 表结构与迁移见 [`migrations`]。铁律：**核心持有数据权威，壳不碰文件系统**。

use std::path::Path;

/// 数据库驱动类型由核心统一出口：**壳无需（也不应）直接依赖驱动**，
/// 这样将来换驱动或换成「核心自有的数据句柄」时，壳一行都不用改。
pub use rusqlite::Connection;

use crate::error::{codes, Error, Result};
use crate::time::now_millis;

pub mod migrations;
mod migrations_v1;
mod migrations_v2;
mod migrations_v3;

/// schema 版本要求的最低 SQLite（trigram tokenizer 自 3.34 起，中文子串匹配要用）。
/// 口径同 `rusqlite::version_number()`：3.34.0 → 3_034_000（i32）。
const MIN_SQLITE: i32 = 3_034_000;

/// 打开（或创建）数据库并完成配置。**不做迁移**——迁移走 [`open_ready`]。
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

/// 打开 + 校验环境 + 跑迁移。这是应用启动的唯一入口。
pub fn open_ready(path: impl AsRef<Path>) -> Result<Connection> {
    let mut conn = open(path)?;
    verify_environment(&conn)?;
    migrations::migrate(&mut conn)?;
    Ok(conn)
}

/// 连接级配置：WAL、外键、忙碌等待。
pub fn configure(conn: &Connection) -> Result<()> {
    // WAL：读写不互相阻塞，崩溃后可恢复（边写边存的前提）
    let mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(Error::unsupported_with(
            codes::WAL_UNAVAILABLE,
            [("mode", mode)],
        ));
    }
    // 外键约束：级联删除靠它（默认是关的，必须显式打开）
    conn.pragma_update(None, "foreign_keys", true)?;
    // NORMAL：WAL 下足够安全，且比 FULL 快得多（0=OFF 1=NORMAL 2=FULL）
    conn.pragma_update(None, "synchronous", 1)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

/// 启动期环境校验——**不满足就明确报错，绝不带病运行**。
///
/// 两件事：SQLite 版本，以及 **FTS5 是否真的可用**。
pub fn verify_environment(conn: &Connection) -> Result<()> {
    let v = rusqlite::version_number();
    if v < MIN_SQLITE {
        return Err(Error::unsupported_with(
            codes::SQLITE_TOO_OLD,
            [("version", rusqlite::version().to_string())],
        ));
    }
    // FTS5：用临时虚拟表做**真实功能探针**（查编译选项在部分构建下不可靠）
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS temp.__fts5_probe USING fts5(x)",
        [],
    )
    .map_err(|e| Error::unsupported_with(codes::FTS5_UNAVAILABLE, [("detail", e.to_string())]))?;
    conn.execute("DROP TABLE temp.__fts5_probe", [])?;
    Ok(())
}

/// 完整性快检：`PRAGMA quick_check` 的结论（`"ok"`，或第一处问题的描述）。
///
/// **只读、不改任何东西**。体检、异常中断之后的判断、自动化脚本都靠它拿一个明确结论——
/// 这样调用方不必自己写 PRAGMA（SQL 只该出现在数据层）。
pub fn quick_check(conn: &Connection) -> Result<String> {
    Ok(conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?)
}

/// 记录一条动作日志（op-log，append-only，为多端同步预留）。
///
/// `device_id` 用调用方传入的本机设备标识；`seq` 由 SQLite 自增保证本机单调。
pub fn append_op(
    conn: &Connection,
    device_id: &str,
    entity: &str,
    entity_id: i64,
    op: &str,
    payload_json: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO op_log(device_id, entity, entity_id, op, payload, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![device_id, entity, entity_id, op, payload_json, now_millis()],
    )?;
    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        (dir, path)
    }

    #[test]
    fn open_ready_creates_schema_and_is_idempotent() {
        let (_dir, path) = tmp_db("a.db");
        {
            let conn = open_ready(&path).unwrap();
            assert_eq!(migrations::user_version(&conn).unwrap(), migrations::schema_version());
        }
        // 再开一次：迁移应幂等，不报错
        let conn = open_ready(&path).unwrap();
        assert_eq!(migrations::user_version(&conn).unwrap(), migrations::schema_version());
        let runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM migration_log WHERE ok=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            runs,
            i64::from(migrations::schema_version()),
            "重复打开不应重复执行迁移：每个迁移只该成功一次"
        );
    }

    #[test]
    fn wal_is_enabled() {
        let (_dir, path) = tmp_db("b.db");
        let conn = open(&path).unwrap();
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn foreign_keys_are_on() {
        let (_dir, path) = tmp_db("c.db");
        let conn = open(&path).unwrap();
        let on: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(on, 1);
    }

    #[test]
    fn fts5_probe_passes_with_bundled_sqlite() {
        let (_dir, path) = tmp_db("d.db");
        let conn = open(&path).unwrap();
        // bundled 自带 SQLite 应满足版本要求
        verify_environment(&conn).expect("bundled SQLite 应满足最低版本且带 FTS5");
    }

    #[test]
    fn op_log_is_append_only_and_monotonic() {
        let (_dir, path) = tmp_db("e.db");
        let conn = open_ready(&path).unwrap();
        let a = append_op(&conn, "dev-1", "work", 1, "create", "{}").unwrap();
        let b = append_op(&conn, "dev-1", "work", 1, "update", "{}").unwrap();
        assert!(b > a, "op-log 序号必须单调递增");
    }
}
