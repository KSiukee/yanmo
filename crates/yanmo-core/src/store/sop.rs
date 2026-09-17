//! SOP：作者**自己的写作流程**——"先干什么、后干什么"这件事本身，也是一件资产。
//!
//! 零件研墨早就齐了（编辑器 / 目录树 / 书架 / 大纲 / 灵感碎片 / 人物卡），缺的是那根
//! 把零件串成"我自己的写法"的线：每次开写都要从零决定先干什么后干什么，那正是
//! 「决策消耗」。这一格存的就是那根线。
//!
//! # 三条定案（用户 2026-09-12 立，2026-09-17 解锁时复核）
//!
//! 1. **跟人，不跟作品**；但允许**按作品覆盖**——沿用外观偏好那套稀疏继承
//!    （全局打底 + 每书覆盖 + 只存改过的项），键是 `sop` / `work.{id}.sop`
//!    （与 `appearance` / `work.{id}.appearance` 同一命名空间）。
//!    **不建表、不迁移**：它就是一格 `settings`。
//! 2. **核心零文案**：核心只说"有哪些步、什么顺序、勾没勾"；步骤的名字与说明是**用户数据**
//!    （作者想怎么改怎么改）。没改过的（`None`）由界面按语言去字典里取
//!    （`sop.step.<id>.*`）。内置那套起手流程只给 [`DEFAULT_STEP_IDS`]——**id 与顺序**。
//! 3. **绝不强制顺序、绝不做打卡**：有"跳过"标记，可以任意跳步 / 回退 / 并行；
//!    库里**不存任何连续天数或达成记录**；`minutes` 只是参考值，不进任何判据。
//!
//! # 两层之间是「不覆盖 / 覆盖」，不是字段级合并
//!
//! 一条 SOP 是**一份有序清单**：哪几步、什么顺序、每步的检查项，都是整份选择的一部分。
//! 所以 `work.{id}.sop` 在就用它、不在才看全局、都不在用内置那套——正是用户拍板的
//! 「选已有 / 创建临时」＝「不覆盖 / 覆盖」两种状态，不为"临时"另造实体。
//! （`appearance` 那种字段级稀疏管的是标量偏好；清单要那么合，"我到底改了哪一步"就说不清了。）
//!
//! 层**内**仍然"只存改过的项"：`SopStep.name = None` = 没改过 → 界面用字典默认文案；
//! 也因此老库读得进来、默认文案改了旧库跟着变，**不必写迁移**。
//!
//! # 版本与回滚
//!
//! 每次改动与它**同一个事务**写一条 op-log（`entity = settings`、`op = set_sop`，
//! payload 是**整份** SOP）——「我什么时候改了流程」可查、两次改动可 diff
//! （[`Store::sop_history`]）、也能回滚到某一次（[`Store::restore_sop`]，它本身
//! 又是一次新改动，**历史不涂改**）。这与快照那条思路一样：留痕 append-only，
//! 回滚是"再写一次"而不是抹掉。

use serde::{Deserialize, Serialize};


/// 一条 SOP 最多几步——手滑粘进一份巨型清单的兜底（真实写法十几步就到头了）。
pub const MAX_STEPS: usize = 40;
/// 稳定 id 最长几个字符（它要拼进界面字典键 `sop.step.<id>`，不能没边）。
pub const MAX_ID_CHARS: usize = 64;
/// 步骤名 / 说明 / 检查项各最长几个字符——那是一句话，不是正文。
pub const MAX_TEXT_CHARS: usize = 200;
/// 参考时长上限（分钟）。它只是参考值，不进任何判据。
pub const MAX_MINUTES: i64 = 24 * 60;

/// 这一版只认的绑定动作：打开哪块面板（`aside`）——面板码用 [`SideTab`](crate::model::SideTab) 的稳定码。
///
/// 「跳到哪一类叩问」暂时接不上：叩问的分类还没定案——
/// 宁可先不编一个将来要改的码表，等它定案再接。
pub const ACTION_ASIDE: &str = "aside";

/// 内置那套起手流程的**步骤 id 与顺序**（核心零文案：名字与说明在界面字典里）。
///
/// 四条，对应作者最常说的那条起步路径：定核心 → 搭大纲 → 写初稿 → 回头改。
/// 它只是**默认**：可以改名、改序、跳过、删掉、自己加步——核心给的是结构，不是规矩。
pub const DEFAULT_STEP_IDS: [&str; 4] = ["core", "outline", "draft", "revise"];

/// 一个检查项：作者自己写的一句话 + 勾没勾。
///
/// `text = None` = 没改过 → 界面用字典里这个 id 的默认文案（与步骤名同一条规矩）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SopCheck {
    /// 稳定 id（界面字典键与"这一步我勾过没有"都靠它）
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 勾没勾（`false` = 默认，与别的默认项一样**不进库**）
    #[serde(default, skip_serializing_if = "is_false")]
    pub done: bool,
}

/// 一步可以绑一个动作：这一版只认"打开哪块面板"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SopAction {
    /// 动作类别（稳定码；这一版只认 `aside`）
    pub target: String,
    /// 动作取值（`aside` 用 [`SideTab`](crate::model::SideTab) 的稳定码）
    pub value: String,
}

/// 一步：id 是稳定身份，名字 / 说明 / 检查项 / 绑定动作 / 跳过标记都跟着它走。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SopStep {
    #[serde(default)]
    pub id: String,
    /// 作者改过的名字；`None` = 没改过 → 界面用字典默认文案
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// 参考时长（分钟）；`None` = 没设过。**只是参考值，不作达标判据**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<SopCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<SopAction>,
    /// 这一篇先不走这一步（**不是删除**：删了就不是他的习惯了）
    #[serde(default, skip_serializing_if = "is_false")]
    pub skipped: bool,
}

/// 存在 `settings` 里的那份 SOP（**稀疏**：没改过的项就是 `None` / 空）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sop {
    #[serde(default)]
    pub steps: Vec<SopStep>,
}

/// 这一份 SOP 是从哪一层来的——界面要说清「这份只属于这一篇」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SopSource {
    /// 还没设过，用的是内置那套起手流程
    Default,
    /// 用的是"我平时的写法"（全局那份）
    Global,
    /// 这一篇单独设过（只属于这篇，没沉淀进习惯库）
    Work,
}

impl SopSource {
    /// 稳定码（进 JSON / 界面查字典；**别改**）。
    pub const fn as_str(self) -> &'static str {
        match self {
            SopSource::Default => "default",
            SopSource::Global => "global",
            SopSource::Work => "work",
        }
    }
}

impl serde::Serialize for SopSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 读出来给人用的那一步（每一项都已落定；`None` 的名字由界面按字典填）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedStep {
    pub id: String,
    pub name: Option<String>,
    pub note: Option<String>,
    pub minutes: Option<i64>,
    pub checks: Vec<SopCheck>,
    pub action: Option<SopAction>,
    pub skipped: bool,
}

/// 读出来给人用的那份 SOP：步骤 + **它是哪一层来的**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedSop {
    pub source: SopSource,
    pub steps: Vec<ResolvedStep>,
}

/// 一次 SOP 改动（op-log 里那一条）——按时间倒着给，好做"改了什么"的对照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SopRevision {
    /// op-log 的序号（**它的版本号就是它**）
    pub seq: i64,
    pub at: i64,
    pub sop: Sop,
}

impl Default for ResolvedSop {
    /// 默认只有一处：内置那套起手流程（id 与顺序），文案留给界面字典。
    fn default() -> Self {
        Self {
            source: SopSource::Default,
            steps: DEFAULT_STEP_IDS
                .iter()
                .map(|id| ResolvedStep {
                    id: (*id).to_string(),
                    name: None,
                    note: None,
                    minutes: None,
                    checks: Vec::new(),
                    action: None,
                    skipped: false,
                })
                .collect(),
        }
    }
}

impl From<Sop> for ResolvedSop {
    fn from(value: Sop) -> Self {
        Self::at(SopSource::Global, value)
    }
}

impl ResolvedSop {
    /// 把某一层的那份收拾成给人用的形状，并标明它来自哪一层。
    pub(super) fn at(source: SopSource, value: Sop) -> Self {
        Self {
            source,
            steps: value.steps.into_iter().map(ResolvedStep::from).collect(),
        }
    }
}

impl From<SopStep> for ResolvedStep {
    fn from(value: SopStep) -> Self {
        Self {
            id: value.id,
            name: value.name,
            note: value.note,
            minutes: value.minutes,
            checks: value.checks,
            action: value.action,
            skipped: value.skipped,
        }
    }
}

/// `false` 是默认值——"只存改过的项"这条对布尔一样成立（不进库，差异也更好读）。
fn is_false(value: &bool) -> bool {
    !*value
}
