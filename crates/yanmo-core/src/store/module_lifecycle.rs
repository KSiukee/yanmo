//! 模块生命周期（注册表这一侧）：**启动许可与卸载**。
//!
//! 单独成件的原因：注册表管"装了谁、给了什么"，这里管"能不能启动、怎么退场"；
//! 两者的变化理由不同（多一个启动来源 vs 多一个注册字段），混在一起会互相拖累。
//!
//! 启动许可的铁律见 [`crate::model::ModuleLaunchSource`]：来源是闭集，没有后台自启；
//! 冻结的模块一张许可都拿不到；链式启动在"可见 + 可断 + 限频"落地前明确拒绝并留痕。

use serde_json::json;

use super::Store;
use crate::error::{codes, Error, Result};
use crate::model::{ModuleLaunchPermit, ModuleLaunchSource};
use crate::time::now_millis;

fn not_found(module_id: &str) -> Error {
    Error::invalid_with(codes::MODULE_NOT_FOUND, [("module_id", module_id.to_string())])
}

impl Store {
    /// **启动许可**：宿主每次拉起模块之前都要向核心要一张。
    ///
    /// 规则（与 [`ModuleLaunchSource`] 的闭集一一对应）：
    /// - 没装 → `module.not_found`；冻结 → `module.disabled`（并留痕 `launch_rejected`）；
    /// - 用户打开 / 用户主动触发 → 放行，留痕 `launch`；
    /// - 被别的模块唤起 → **现在一律拒**（`module.chain_unavailable`，留痕）：
    ///   链要等"可见 + 可断 + 频率上限"那一族落地，不能先放一条看不见的链进来。
    ///
    /// 许可**不是可重放的令牌**：每次启动前重新要；要不到就不许启动。
    pub fn authorize_module_launch(
        &mut self,
        module_id: &str,
        source: &ModuleLaunchSource,
    ) -> Result<ModuleLaunchPermit> {
        source.validate()?;
        let record = self.module_record(module_id)?;
        if !record.is_active() {
            self.log_rejected_launch(module_id, source, "frozen");
            return Err(disabled(module_id));
        }
        if let ModuleLaunchSource::ModuleChain { from } = source {
            let caller = self.module_record(from)?;
            if !caller.is_active() {
                self.log_rejected_launch(module_id, source, "caller_frozen");
                return Err(disabled(from));
            }
            self.log_rejected_launch(module_id, source, "chain_unavailable");
            return Err(Error::invalid(codes::MODULE_CHAIN_UNAVAILABLE));
        }

        let now = now_millis();
        let tx = self.conn.transaction()?;
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            "launch",
            launch_payload(module_id, source, None),
        )?;
        tx.commit()?;
        Ok(ModuleLaunchPermit {
            module_id: module_id.to_string(),
            source: source.clone(),
            granted_at: now,
        })
    }

    /// 拒绝一次启动并留痕（**审计尽力而为**：写不进去也不改返回给调用方的原因）。
    fn log_rejected_launch(
        &mut self,
        module_id: &str,
        source: &ModuleLaunchSource,
        reason: &str,
    ) {
        let Ok(tx) = self.conn.transaction() else {
            return;
        };
        let _ = Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            "launch_rejected",
            launch_payload(module_id, source, Some(reason)),
        );
        let _ = tx.commit();
    }

    /// 卸载：**只从注册表移除，模块数据默认保留**。
    ///
    /// 数据是作者自己的东西，卸载不等于"连数据一起删"；真要删数据走
    /// [`crate::store::ModuleData::drop_namespace`]，由界面在二次确认之后显式调用。
    /// 卸载正在运行的模块前，宿主必须先把它停干净（宿主还没做，见模块生命周期文档）。
    pub fn uninstall_module(&mut self, module_id: &str) -> Result<()> {
        let record = self.module_record(module_id)?;
        let tx = self.conn.transaction()?;
        let changed = tx.execute("DELETE FROM modules WHERE id = ?1", [module_id])?;
        if changed == 0 {
            return Err(not_found(module_id));
        }
        tx.execute(
            "DELETE FROM module_credentials WHERE module_id = ?1",
            [module_id],
        )?;
        Self::record_in(
            &self.device_id,
            &tx,
            "modules",
            0,
            "uninstall",
            json!({
                "module_id": module_id,
                "version": record.manifest.version,
                "data_kept": true,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn disabled(module_id: &str) -> Error {
    Error::invalid_with(codes::MODULE_DISABLED, [("module_id", module_id.to_string())])
}

/// 启动留痕的 payload（成功与拒绝共用；`reason` 只有拒绝时才有）。
fn launch_payload(
    module_id: &str,
    source: &ModuleLaunchSource,
    reason: Option<&str>,
) -> serde_json::Value {
    let mut payload = json!({ "module_id": module_id, "source": source.as_str() });
    if let Some(reason) = reason {
        payload["reason"] = json!(reason);
    }
    match source {
        ModuleLaunchSource::UserOpen => {}
        ModuleLaunchSource::UserAction { action } => {
            payload["action"] = json!(action);
        }
        ModuleLaunchSource::ModuleChain { from } => {
            payload["from"] = json!(from);
        }
    }
    payload
}
