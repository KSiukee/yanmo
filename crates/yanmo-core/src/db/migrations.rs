//! schema 版本迁移框架。
//!
//! # 迁移铁律
//!
//! 1. **列表只增不改**：数据库结构变更必须在 [`MIGRATIONS`](crate::db::migrations::MIGRATIONS) 追加新版本，
//!    禁止手改已有表结构、禁止改动已发布的 step。
//! 2. **原子化**：多语句在**显式事务**内**逐条 `execute`**；
//!    ⚠️ **不使用 `executescript` / `execute_batch`**——那类批量接口可能隐式提交，
//!    外层事务保护失效，中途失败会留下**半迁移状态**（表已建/列已加但版本未更新）。
//! 3. **幂等化**：`ALTER TABLE ADD COLUMN` 前先查 `PRAGMA table_info` 判存在。
//! 4. **留痕**：每次迁移尝试都写 `migration_log`，**成功失败都记**。
//! 5. **拒绝降级**：库版本高于引擎支持时**明确报错**，绝不猜着读。

use rusqlite::Connection;

use super::{
    migrations_v1, migrations_v2, migrations_v3, migrations_v4, migrations_v5, migrations_v6,
    migrations_v7, migrations_v8, migrations_v9, migrations_v10, migrations_v11,
    migrations_v12,
};
use crate::error::{Error, Result};
use crate::time::now_millis;

/// 一次 schema 迁移。
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    /// 逐条执行的 SQL。**每条必须是一个完整语句**（不是脚本）。
    pub steps: &'static [&'static str],
    /// **要先看库才能决定**的步骤（如 `ALTER TABLE ADD COLUMN` 前判列是否存在）。
    ///
    /// 它在事务内、静态步骤之前被调用，产出的语句同样**逐条 execute**——
    /// 框架的原子化纪律不打折，只是把"要不要执行"这一步交给代码判（铁律 3：幂等化）。
    pub prepare: Option<fn(&rusqlite::Connection) -> Result<Vec<String>>>,
}

/// 迁移列表——**只增不改**。
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        steps: migrations_v1::STEPS,
        prepare: None,
    },
    Migration {
        version: 2,
        name: "search_index",
        steps: migrations_v2::STEPS,
        prepare: None,
    },
    Migration {
        version: 3,
        name: "snapshot_pinned",
        steps: migrations_v3::STEPS,
        prepare: Some(migrations_v3::prepare),
    },
    Migration {
        version: 4,
        name: "work_language",
        steps: migrations_v4::STEPS,
        prepare: Some(migrations_v4::prepare),
    },
    Migration {
        version: 5,
        name: "node_count_calibers",
        steps: migrations_v5::STEPS,
        prepare: Some(migrations_v5::prepare),
    },
    Migration {
        version: 6,
        name: "outline_summaries",
        steps: migrations_v6::STEPS,
        prepare: Some(migrations_v6::prepare),
    },
    Migration {
        version: 7,
        name: "writing_days",
        steps: migrations_v7::STEPS,
        prepare: None,
    },
    Migration {
        version: 8,
        name: "fragment_question_card",
        steps: migrations_v8::STEPS,
        prepare: Some(migrations_v8::prepare),
    },
    Migration {
        version: 9,
        name: "question_selection",
        steps: migrations_v9::STEPS,
        prepare: Some(migrations_v9::prepare),
    },
    Migration {
        version: 10,
        name: "question_deferrals",
        steps: migrations_v10::STEPS,
        prepare: None,
    },
    Migration {
        version: 11,
        name: "entity_and_scene_cards",
        steps: migrations_v11::STEPS,
        prepare: None,
    },
    Migration {
        version: 12,
        name: "foreshadows_and_story_time",
        steps: migrations_v12::STEPS,
        prepare: Some(migrations_v12::prepare),
    },
];

/// 最新 schema 版本（**派生自迁移表末位**，不手写——防漂移）。
pub fn schema_version() -> u32 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}

/// 读库里的 `user_version`（= 已应用的 schema 版本）。
pub fn user_version(conn: &Connection) -> Result<u32> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    Ok(v.max(0) as u32)
}

/// 把库升到最新版本。已是最新则什么都不做（幂等）。
pub fn migrate(conn: &mut Connection) -> Result<u32> {
    ensure_log_table(conn)?;
    let current = user_version(conn)?;
    let latest = schema_version();
    if current > latest {
        // ★ 库比引擎新时也不能猜着读——明确报错并交给用户升级，而不是按旧结构硬读
        return Err(Error::SchemaTooNew {
            found: current,
            supported: latest,
        });
    }
    for m in MIGRATIONS.iter().filter(|m| m.version > current) {
        apply(conn, m)?;
    }
    Ok(latest)
}

/// `migration_log` 是迁移框架自己的表，不属于任何编号迁移（先有鸡才有蛋）。
fn ensure_log_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS migration_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            version     INTEGER NOT NULL,
            name        TEXT    NOT NULL,
            started_at  INTEGER NOT NULL,
            finished_at INTEGER,
            ok          INTEGER NOT NULL DEFAULT 0,
            error       TEXT    NOT NULL DEFAULT ''
        )",
        [],
    )?;
    Ok(())
}

/// 执行单个迁移：查日志 → 记开始 → 事务内逐条跑 → 提交并更新版本 → 记结果。
fn apply(conn: &mut Connection, m: &Migration) -> Result<()> {
    conn.execute(
        "INSERT INTO migration_log(version, name, started_at, ok) VALUES(?1, ?2, ?3, 0)",
        rusqlite::params![m.version, m.name, now_millis()],
    )?;
    let log_id = conn.last_insert_rowid();

    let tx = conn.transaction()?;
    // 先跑"按需产出"的步骤（如判过存在的 ALTER），再跑静态步骤——两者同样逐条 execute
    let prepared = match m.prepare {
        Some(build) => build(&tx)?,
        None => Vec::new(),
    };
    for step in m.steps.iter().map(|s| (*s).to_string()).chain(prepared) {
        if let Err(e) = tx.execute(step.as_str(), []) {
            drop(tx); // 回滚：绝不留下半迁移状态
            record_failure(conn, log_id, &e.to_string())?;
            return Err(Error::Db(e));
        }
    }
    // user_version 也参与事务（SQLite 的 user_version 写在库头，是事务性的）
    tx.pragma_update(None, "user_version", m.version)?;
    tx.commit()?;

    conn.execute(
        "UPDATE migration_log SET finished_at=?1, ok=1, error='' WHERE id=?2",
        rusqlite::params![now_millis(), log_id],
    )?;
    Ok(())
}

fn record_failure(conn: &Connection, log_id: i64, err: &str) -> Result<()> {
    conn.execute(
        "UPDATE migration_log SET finished_at=?1, ok=0, error=?2 WHERE id=?3",
        rusqlite::params![now_millis(), err, log_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_list_is_append_only_and_versioned_1_to_n() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1, "迁移版本必须从 1 连续递增");
            assert!(
                !m.steps.is_empty() || m.prepare.is_some(),
                "迁移 {} 既没有静态步骤也没有按需步骤",
                m.name
            );
        }
    }

    #[test]
    fn schema_version_derives_from_last_migration() {
        assert_eq!(schema_version(), MIGRATIONS.last().unwrap().version);
    }

    #[test]
    fn refuses_to_open_newer_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.db");
        {
            let mut conn = crate::db::open(&path).unwrap();
            migrate(&mut conn).unwrap();
            // 伪造成"未来版本"的库
            conn.pragma_update(None, "user_version", schema_version() + 1).unwrap();
        }
        let mut conn = crate::db::open(&path).unwrap();
        let err = migrate(&mut conn).unwrap_err();
        assert!(matches!(err, Error::SchemaTooNew { .. }), "应明确拒绝而不是猜着读");
    }

    #[test]
    fn failure_is_recorded_and_version_not_bumped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fail.db");
        let mut conn = crate::db::open(&path).unwrap();
        ensure_log_table(&conn).unwrap();

        // 故意构造一个会失败的迁移（语法错误）
        static BAD: &[&str] = &["CREATE TABLE ok_one(x INTEGER)", "THIS IS NOT SQL"];
        let bad = Migration { version: 1, name: "bad", steps: BAD, prepare: None };

        assert!(apply(&mut conn, &bad).is_err());
        assert_eq!(user_version(&conn).unwrap(), 0, "失败不得推进版本号");

        let (ok, err): (i64, String) = conn
            .query_row("SELECT ok, error FROM migration_log ORDER BY id DESC LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(ok, 0);
        assert!(!err.is_empty(), "失败原因必须留痕");

        // 事务回滚：半迁移的表不应存在
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ok_one'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 0, "失败必须整体回滚，不留半迁移状态");
    }
}
