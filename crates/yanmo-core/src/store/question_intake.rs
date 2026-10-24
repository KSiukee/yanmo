//! 模块叩问的**唯一入口**：验凭据、验依据、限频；丢弃也留痕。
//!
//! 规则（模块通道定案）：
//!
//! ① **必须标来源**：卡片的 `source` 就是模块 id（核心自带是 [`CORE_SOURCE`]）；
//! ② **限频**：每模块有窗口上限与冷却，超限**丢弃**（不排队堆积）；
//! ③ **未声明 / 未激活一次都提不进来**：凭据由宿主按能力清单与生命周期填；
//! ④ **静音只静音它一个**：按来源静音的实现在 [`super::question_dispose`]，本入口只管写对来源；
//! ⑤ **不得绕过**：核心数据对模块只读，模块拿不到库句柄；这条入口是唯一会
//!    给问题卡盖上「某个模块」的来源章的地方；
//! ⑥ **有据才问**：至少一条依据锚点，否则丢弃；
//! ⑦ **不抢占**：模块卡与核心卡同池同界面，但**主动推的那一次优先核心**
//!    （见 [`super::question_push`]）。
//!
//! # 频率账放在哪
//!
//! 与 `question.push_state` / `question.muted_sources` 同一手法：`settings` 里的一个 JSON 键，
//! **不新建表、不动迁移**。建卡与记账**同一个事务**——要么"问进来了并且记了数"，
//! 要么两样都不发生（否则模块能靠失败重试绕过上限）。
//!
//! # 丢弃也要有记录
//!
//! 丢弃不是错误（模块不该因为被限频而崩溃），但**必须留痕**：op-log 里一条
//! `entity = question_intake / op = drop`，带模块 id 与原因。记录按模块节流
//! （默认一分钟最多一条），免得一个坏模块把日志刷爆；**第一次一定记**。

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Store;
use crate::error::Result;
use crate::model::{
    clean_basis, is_core_source, is_valid_module_id, validate_draft, ModuleQuestion,
    ModuleQuestionGrant, NewQuestionCard, QuestionDropReason, MAX_MODULE_ID_CHARS,
};
use crate::time::now_millis;

/// 频率账在 `settings` 里的键。
const INTAKE_STATE_KEY: &str = "question.module_intake";

/// 最多记住多少个模块的频率账（超了先丢最久没动过的；正常的装机量远小于这个数）。
const MAX_TRACKED_MODULES: usize = 128;

/// 一条频率账保留多久（过期且没再动过的顺手清掉）。
const STATE_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// 限频参数。生产走 [`Default`]（保守）；测试可以注入更小的窗口把规则跑一遍。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleIntakePolicy {
    /// 计数窗口（毫秒）：窗口内最多 `limit` 条
    pub window_ms: i64,
    /// 窗口内最多几条
    pub limit: u32,
    /// 两条之间至少隔多久（毫秒）
    pub cooldown_ms: i64,
    /// 同一个模块的"丢弃留痕"最多多久写一条（毫秒；0 = 每次都记）
    pub drop_log_interval_ms: i64,
}

impl Default for ModuleIntakePolicy {
    /// 保守的默认值：**每小时最多 4 条、两条之间至少隔 1 分钟**。
    ///
    /// 叩问是"在写作途中打扰一下"的东西，不是消息流；4 条已经比核心自己的推送
    /// 配额（默认每天 3 次）宽，足够一个正常的模块工作。等模块管理页
    /// 落地后再把这两个数暴露成可调项（那时才有"谁调的、调成多少"的地方）。
    fn default() -> Self {
        Self {
            window_ms: 60 * 60 * 1000,
            limit: 4,
            cooldown_ms: 60 * 1000,
            drop_log_interval_ms: 60 * 1000,
        }
    }
}

/// 一次模块叩问的结果（稳定码进命令行与测试；界面按 `code` 分支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum QuestionIntakeOutcome {
    /// 收下了：卡已经进池（`source` 是模块 id）
    Accepted { card_id: i64 },
    /// 丢弃了（原因见 [`QuestionDropReason`]）；**不是错误**，模块不必重试
    Dropped { reason: QuestionDropReason },
}

/// 一个模块的频率账。
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
struct IntakeSlot {
    #[serde(default)]
    window_start: i64,
    #[serde(default)]
    accepted: u32,
    #[serde(default)]
    last_at: i64,
    #[serde(default)]
    last_drop_log_at: i64,
}

type IntakeState = BTreeMap<String, IntakeSlot>;

fn read_state(conn: &Connection) -> Result<IntakeState> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![INTAKE_STATE_KEY],
            |row| row.get(0),
        )
        .optional()?;
    // 坏 JSON 当"没有任何账"：宁可多放一条过去，也不要因为一条坏记录把通道关死。
    // 但**读库失败要报错**（那与坏记录不是一回事：读不到就无从判断限频）。
    Ok(raw.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default())
}

fn write_state(conn: &Connection, state: &IntakeState) -> Result<()> {
    let json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
        params![INTAKE_STATE_KEY, json, now_millis()],
    )?;
    Ok(())
}

/// 清掉太久没动过的账，并压到上限之内（先丢最久没动的）。
fn prune(state: &mut IntakeState, now: i64) {
    state.retain(|_, slot| {
        let newest = slot.last_at.max(slot.last_drop_log_at).max(slot.window_start);
        now.saturating_sub(newest) < STATE_TTL_MS
    });
    if state.len() <= MAX_TRACKED_MODULES {
        return;
    }
    let mut order: Vec<(String, i64)> = state
        .iter()
        .map(|(key, slot)| (key.clone(), slot.last_at.max(slot.last_drop_log_at)))
        .collect();
    order.sort_by_key(|(_, at)| *at);
    for (key, _) in order.into_iter().take(state.len() - MAX_TRACKED_MODULES) {
        state.remove(&key);
    }
}

/// 丢弃记录用的 key：空 id 也要有一个可读的记号；超长的截到上限（这只是账本的键）。
fn drop_key(module_id: &str) -> String {
    let value = module_id.trim();
    if value.is_empty() {
        "<empty>".to_string()
    } else {
        value.chars().take(MAX_MODULE_ID_CHARS).collect()
    }
}

impl Store {
    /// 模块提交一条叩问（生产默认限频：每小时 4 条、间隔 1 分钟）。
    pub fn submit_module_question(
        &mut self,
        grant: &ModuleQuestionGrant,
        draft: ModuleQuestion,
    ) -> Result<QuestionIntakeOutcome> {
        self.submit_module_question_with(grant, draft, ModuleIntakePolicy::default())
    }

    /// 同上，但可以换一套限频参数（测试 / 将来的模块管理页用）。
    pub fn submit_module_question_with(
        &mut self,
        grant: &ModuleQuestionGrant,
        draft: ModuleQuestion,
        policy: ModuleIntakePolicy,
    ) -> Result<QuestionIntakeOutcome> {
        let module_id = grant.module_id.trim();

        // ── ① 凭据：先问"声明过没有"，再问"现在激活吗"（前者更接近根因）──
        if !grant.may_ask {
            return self.drop_module_question(module_id, draft.work_id, QuestionDropReason::NotDeclared, policy);
        }
        if !grant.active {
            return self.drop_module_question(module_id, draft.work_id, QuestionDropReason::Inactive, policy);
        }
        // ── ② 来源：不许冒充核心；id 本身要能安全地进 source 列与界面 ──
        if is_core_source(module_id) {
            return self.drop_module_question(module_id, draft.work_id, QuestionDropReason::SourceReserved, policy);
        }
        if !is_valid_module_id(module_id) {
            return self.drop_module_question(module_id, draft.work_id, QuestionDropReason::SourceInvalid, policy);
        }

        // ── ③ 内容与依据：正文/重要度与核心建卡同一套拒绝理由；依据至少一条 ──
        validate_draft(&draft)?;
        let basis = clean_basis(&draft.linked);
        if basis.is_empty() {
            return self.drop_module_question(module_id, draft.work_id, QuestionDropReason::NoBasis, policy);
        }

        // ── ④ 频率：窗口 + 冷却 ──
        let now = now_millis();
        let mut state = read_state(&self.conn)?;
        let slot = state.entry(module_id.to_string()).or_default();
        if slot.window_start == 0 || now.saturating_sub(slot.window_start) >= policy.window_ms {
            slot.window_start = now;
            slot.accepted = 0;
        }
        let cooling = slot.last_at > 0 && now.saturating_sub(slot.last_at) < policy.cooldown_ms;
        if cooling || slot.accepted >= policy.limit {
            return self.drop_module_question(module_id, draft.work_id, QuestionDropReason::RateLimited, policy);
        }

        // ── ⑤ 建卡 + 记数：同一个事务（失败重试绕不过上限）──
        let card = NewQuestionCard {
            work_id: draft.work_id,
            body: draft.body.trim().to_string(),
            source: module_id.to_string(),
            template_key: String::new(),
            importance: draft.importance,
            linked: basis,
            derived_from: None,
            auto_derived: false,
        };
        let tx = self.conn.transaction()?;
        let card_id = Self::create_question_card_in(&self.device_id, &tx, &card)?;
        slot.accepted += 1;
        slot.last_at = now;
        write_state(&tx, &state)?;
        tx.commit()?;
        Ok(QuestionIntakeOutcome::Accepted { card_id })
    }

    /// 丢弃 + 留痕（按模块节流；**第一次一定记**）。
    fn drop_module_question(
        &mut self,
        module_id: &str,
        work_id: i64,
        reason: QuestionDropReason,
        policy: ModuleIntakePolicy,
    ) -> Result<QuestionIntakeOutcome> {
        let now = now_millis();
        let key = drop_key(module_id);
        let mut state = read_state(&self.conn)?;
        let slot = state.entry(key.clone()).or_default();
        let should_log = policy.drop_log_interval_ms <= 0
            || slot.last_drop_log_at == 0
            || now.saturating_sub(slot.last_drop_log_at) >= policy.drop_log_interval_ms;
        if should_log {
            let tx = self.conn.transaction()?;
            Self::record_in(
                &self.device_id,
                &tx,
                "question_intake",
                work_id,
                "drop",
                json!({ "module_id": key, "reason": reason.as_str() }),
            )?;
            slot.last_drop_log_at = now;
            prune(&mut state, now);
            write_state(&tx, &state)?;
            tx.commit()?;
        }
        Ok(QuestionIntakeOutcome::Dropped { reason })
    }
}
