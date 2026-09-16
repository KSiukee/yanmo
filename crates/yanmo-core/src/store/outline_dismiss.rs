//! 「这一处我知道了」：忽略标记的读写。
//!
//! 冲突检测是**每次打开都重扫**的（数据可能一直在变），所以"作者看过、不想再看"这件事
//! 必须**记下来**，否则同一处会永远排在清单最前面——被无视的提醒等于没有提醒。
//!
//! # 为什么存在 `settings` 里，而不是再开一张表
//!
//! 它是一小把**不透明记号**（每条问题一个指纹，见 `outline::OutlineIssue::fingerprint`）：
//! 没有字段要单独查、没有时间线要排、也没有跨表关系——整把读出来、整把写回去就够用。
//! 另开一张表只会多一个迁移与一个写入口；`settings` 本来就是"键值设置"那张表。
//! 键按书分：`work.{id}.outline.dismissed`（与 `work.{id}.appearance` 同一条命名）。
//!
//! 三条分寸：
//!
//! 1. **幂等**：同一个指纹忽略两次不算错（界面上连点两下不该报错）；
//! 2. **认不出的当没忽略**：库里那把记号里如果有本版本不认识的规则，
//!    读的时候照收（撤销要用），但**不会**让任何一条问题凭空消失——
//!    消失的前提是"指纹真的对上了"；
//! 3. **留痕**：忽略与撤销都写 op-log（这是作者对自己清单的处置，可追溯）。

use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::Store;
use crate::error::Result;
use crate::time::now_millis;

/// 这本书的忽略标记存在哪个键下（按书分：换一本书是另一套清单）。
fn dismiss_key(work_id: i64) -> String {
    format!("work.{work_id}.outline.dismissed")
}

impl Store {
    /// 这本书已经忽略掉的指纹（**原样读回来**，去重、保序）。
    pub fn dismissed_issues(&self, work_id: i64) -> Result<Vec<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![dismiss_key(work_id)],
                |row| row.get(0),
            )
            .optional()?;
        // 坏 JSON 当"一个都没忽略"：读不出来就当没记过（界面会重新报一遍，那比卡住强）
        let mut out: Vec<String> = raw
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default();
        out.dedup();
        Ok(out)
    }

    /// 忽略一条（幂等），返回**写完之后的那把记号**。
    pub fn dismiss_issue(
        &mut self,
        work_id: i64,
        fingerprint: &str,
        trigger: &str,
    ) -> Result<Vec<String>> {
        let mut current = self.dismissed_issues(work_id)?;
        if current.iter().any(|item| item == fingerprint) {
            return Ok(current);
        }
        current.push(fingerprint.to_string());
        self.write_dismissed(work_id, &current, "dismiss", fingerprint, trigger)?;
        Ok(current)
    }

    /// 撤销一次忽略（**回头路**：清单上"我忽略过的"那一块要能捡回来）。
    pub fn undismiss_issue(
        &mut self,
        work_id: i64,
        fingerprint: &str,
        trigger: &str,
    ) -> Result<Vec<String>> {
        let current = self.dismissed_issues(work_id)?;
        let kept: Vec<String> = current
            .into_iter()
            .filter(|item| item != fingerprint)
            .collect();
        self.write_dismissed(work_id, &kept, "undismiss", fingerprint, trigger)?;
        Ok(kept)
    }

    /// 全部重新看一遍（"我改过设定了，从头再扫给我看"）。
    pub fn clear_dismissed_issues(&mut self, work_id: i64, trigger: &str) -> Result<Vec<String>> {
        self.write_dismissed(work_id, &[], "clear_dismissed", "", trigger)?;
        Ok(Vec::new())
    }

    /// 落盘一把记号 + 留痕（同一个事务）。
    fn write_dismissed(
        &mut self,
        work_id: i64,
        fingerprints: &[String],
        op: &str,
        fingerprint: &str,
        trigger: &str,
    ) -> Result<()> {
        let now = now_millis();
        let tx = self.conn.transaction()?;
        let value = serde_json::to_string(fingerprints).unwrap_or_else(|_| "[]".to_string());
        if fingerprints.is_empty() {
            // 空清单就把键删掉（与偏好那条规矩一致：不留空记录）
            tx.execute("DELETE FROM settings WHERE key = ?1", params![dismiss_key(work_id)])?;
        } else {
            tx.execute(
                "INSERT INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![dismiss_key(work_id), value, now],
            )?;
        }
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id,
            op,
            json!({ "fingerprint": fingerprint, "count": fingerprints.len(), "trigger": trigger }),
        )?;
        tx.commit()?;
        Ok(())
    }
}
