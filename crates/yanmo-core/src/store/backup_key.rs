//! 加密备份包的**钥匙那一格**：恢复码存在哪儿、怎么取。
//!
//! # 为什么恢复码要存在本机（而口令不存）
//!
//! 每一份备份包都用**同一把**恢复码包住它的数据钥匙——不然作者抄在纸上那一串，只能开
//! 「抄下来的那天」那一份，后面的备份全都开不了（那是把人往数据丢失上送）。
//! 所以恢复码必须跨会话稳定，也就必须落盘；**口令相反**：它只在内存里活着
//! （应用关掉就没了，下次要重输），这是它作为第一把钥匙该有的样子。
//!
//! 这样会不会把加密削弱成摆设？**不会**：能读到这台机器上这份偏好的攻击者，
//! 本来就能直接读明文稿库（见 SECURITY.md 的威胁模型表：机器级对手明确不在防护范围内）。
//! 恢复码真正要防的是「作者忘了口令」——那件事上它是唯一的路。
//!
//! # 一条不能忘的提醒（界面要说给作者听）
//!
//! **恢复码在备份包里是取不回来的**：包里那一份要先解开才看得到。
//! 所以「抄到别处」不是客套话——只有本机这一份、又忘了口令，那份包就打不开了。
//!
//! 存在 `settings` 里一个单独的键上（**不放进 [`super::backup::BackupConfig`]**）：
//! 那份偏好会被界面整份读走，恢复码不该跟着它到处走。

use rusqlite::{params, OptionalExtension};

use super::Store;
use crate::error::Result;
use crate::time::now_millis;

/// 恢复码在 `settings` 里的键。
const RECOVERY_KEY: &str = "backup.seal_recovery";

impl Store {
    /// 记着的那把恢复码（没开过加密就是 `None`）。
    pub fn seal_recovery(&self) -> Result<Option<String>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![RECOVERY_KEY], |r| r.get(0))
            .optional()?;
        Ok(raw.filter(|code| !code.trim().is_empty()))
    }

    /// 记下这把恢复码（开启加密时由壳生成一次就写进来）。
    ///
    /// **只在还没有的时候写**：已经有一把就不动它——换掉它等于让以前那些包失去第二把钥匙
    /// （作者纸上那一串还是旧的，谁也开不了）。确实要换，得走「显式重建」那条路（将来再说）。
    pub fn set_seal_recovery(&mut self, code: &str) -> Result<()> {
        if self.seal_recovery()?.is_some() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![RECOVERY_KEY, code, now_millis()],
        )?;
        Ok(())
    }

    /// 关掉加密时**不动**恢复码：以前那些包还要靠它开（作者纸上那一串也是它）。
    /// 真想抹掉，那是一次「我确定以后都不需要这些包了」的独立动作，不跟开关绑在一起。
    pub fn clear_seal_recovery(&mut self) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", params![RECOVERY_KEY])?;
        Ok(())
    }
}
