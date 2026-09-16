//! 外观与写作行为的偏好：**全局打底 + 每书可选覆盖**。
//!
//! 三条规矩：
//! 1. **只存改过的项**：库里存的是"空档"（`None` = 没改过），读的时候才落默认值——
//!    将来默认值改了，没动过这一项的旧库也跟着变，不必写迁移。
//! 2. **书的覆盖盖在全局上**：每本书的键只在作者真的"单独设过"时才存在；没设就继承全局。
//! 3. **读不出来的记录当没设过**：宁可回默认，也不要让一个坏 JSON 把界面卡住。
//!
//! 与正文无关：这些偏好**不进导出**，也不改动数据的任何字节。
//!
//! # 为什么把「字数口径」也放这里
//!
//! 它看着像"统计"，其实是**显示偏好**：算哪三个数是核心的事（`text::WordCaliber`），
//! 眼前显示哪一个由作者选。放进同一份设置是为了**共用这一套"全局 + 每书覆盖"的机制**——
//! 再起一个模块就是第二份一模一样的读/写/合并/重置代码（重复副本＝拆分硬信号）。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::Result;
use crate::model::{ChapterNumbering, NamingStyle, QuestionTone};
use crate::text::WordCaliber;
use crate::time::now_millis;
use crate::typeset::QuoteStyle;

/// 每日目标的上限：手滑多打几个零的兜底（一天 100 万字已远超任何人的手速）。
const MAX_DAILY_GOAL: i64 = 1_000_000;

/// 主动问一句的默认配额与冷却（与 `question::push` 的口径一致：稀缺才好）。
const DEFAULT_PUSH_PER_DAY: i64 = 3;
const DEFAULT_PUSH_COOLDOWN_MINUTES: i64 = 60;
/// 手滑兜底：一天最多问 20 次、冷却最长一天（再多就不叫"稀缺"了）。
const MAX_PUSH_PER_DAY: i64 = 20;
const MAX_PUSH_COOLDOWN_MINUTES: i64 = 24 * 60;

/// 次数类偏好的夹取：没设过用默认；设了就夹进 `0..=max`（0 是正经取值＝关掉那件事）。
fn clamp_count(value: Option<i64>, fallback: i64, max: i64) -> i64 {
    match value {
        None => fallback,
        Some(v) => v.clamp(0, max),
    }
}

/// 作者改过的项（`None` = 没改过，用默认）。
///
/// 加新项就往这里加一个 `Option<...>` 字段：老库读得进来（缺的字段算没改过），
/// 新库被老版本读到也只是多一个不认识的字段——**不必写迁移**。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    /// 打开"最新那一章"时，跳到段末并聚焦输入光标
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_to_end_on_latest: Option<bool>,
    /// 状态栏显示哪个字数口径（`text::WordCaliber` 的稳定代码）。
    ///
    /// `None` = 没改过 → **跟作品语言走**（中文逐字 / 英文按词 / 日文逐字），
    /// 由调用方用 `WorkLanguage::default_caliber()` 落定——核心这里不替它猜。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_count_caliber: Option<String>,
    /// 引号用哪一套（`typeset::QuoteStyle` 的稳定代码：`curly` / `corner`）。
    ///
    /// `None` = 没改过 → 默认弯引号；排版清理时用它，作者选一次就记住。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_style: Option<String>,
    /// 每日码字目标（`None` = 没设过 / 不想设）。
    ///
    /// 数字的含义跟着**状态栏当前的字数口径**走（逐字 / 无标点 / 按词）——
    /// 口径本身是另一项偏好，这里只存"多少个字"，不替作者把两件事绑死。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_goal: Option<i64>,
    /// 新建条目用什么命名规则（`NamingStyle` 的稳定代码）。
    ///
    /// `None` = 没改过 → **跟作品类型走**（长篇给号、单篇与文集不编号），
    /// 由 [`Store::naming_style`] 落定——核心这里不替它猜。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<String>,
    /// 章的号跨不跨卷数（`ChapterNumbering` 的稳定代码：`continue` / `per_volume`）。
    ///
    /// `None` = 没改过 → 跨卷延续（默认）。它只影响**渲染时怎么数**，
    /// 标题里存的还是模板，所以改一下立刻全见效、正文一个字不动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_numbering: Option<String>,
    /// 正文字号（px；`None` = 没改过，用界面默认那档）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_font_size: Option<i64>,
    /// 正文行距（百分比：190 = 1.9 倍；`None` = 没改过）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_line_height: Option<i64>,
    /// 正文字距（em 的百分之几：5 = 0.05em；`None` = 没改过）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_letter_spacing: Option<i64>,
    /// 叩问问话的语气（`QuestionTone` 的稳定代码：warm / neutral / direct）。
    ///
    /// `None` = 没改过 → **温柔**那档。（`neutral` 就是"不加语气"，也就是把这层关掉。）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_tone: Option<String>,
    /// 写的时候最多主动问几次（**0 = 不打扰**）。
    ///
    /// 它**只限"推"**：用完了，面板照样能开、问题照样能翻——作者主动要看的不受配额管。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_push_per_day: Option<i64>,
    /// 两次主动问之间至少隔多少分钟（冷却：刚问过就别再冒头）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_push_cooldown_minutes: Option<i64>,
}

/// 正文排版的**可读范围**：超出就夹住（手滑打 1000 号字不该把版面炸了，也不该报错挡人）。
const FONT_SIZE_RANGE: (i64, i64) = (12, 30);
const LINE_HEIGHT_RANGE: (i64, i64) = (110, 260);
const LETTER_SPACING_RANGE: (i64, i64) = (0, 20);

/// 夹进范围；`≤0` 当"没设过"（界面用 0 表达"回默认"）。
///
/// 只服务正文排版这三项：它们**纯观感**——不进导出、不动正文一个字节（有验收钉着）。
fn typography_value(value: i64, (low, high): (i64, i64)) -> Option<i64> {
    (value > 0).then(|| value.clamp(low, high))
}

impl Appearance {
    /// 一项都没改过——那就没必要在库里留这个键。
    fn is_empty(&self) -> bool {
        self.jump_to_end_on_latest.is_none()
            && self.word_count_caliber.is_none()
            && self.quote_style.is_none()
            && self.daily_goal.is_none()
            && self.naming.is_none()
            && self.chapter_numbering.is_none()
            && self.editor_font_size.is_none()
            && self.editor_line_height.is_none()
            && self.editor_letter_spacing.is_none()
            && self.question_tone.is_none()
            && self.question_push_per_day.is_none()
            && self.question_push_cooldown_minutes.is_none()
    }

    /// 把 `over`（书的覆盖）盖在 `self`（全局）上：**只覆盖它真设过的项**。
    fn overridden_by(&self, over: &Appearance) -> Appearance {
        Appearance {
            jump_to_end_on_latest: over.jump_to_end_on_latest.or(self.jump_to_end_on_latest),
            word_count_caliber: over
                .word_count_caliber
                .clone()
                .or_else(|| self.word_count_caliber.clone()),
            quote_style: over.quote_style.clone().or_else(|| self.quote_style.clone()),
            daily_goal: over.daily_goal.or(self.daily_goal),
            naming: over.naming.clone().or_else(|| self.naming.clone()),
            chapter_numbering: over
                .chapter_numbering
                .clone()
                .or_else(|| self.chapter_numbering.clone()),
            // 排版三项：每书覆盖也走同一条路（第一版界面只暴露全局，机制先留着）
            editor_font_size: over.editor_font_size.or(self.editor_font_size),
            editor_line_height: over.editor_line_height.or(self.editor_line_height),
            editor_letter_spacing: over.editor_letter_spacing.or(self.editor_letter_spacing),
            question_tone: over.question_tone.clone().or_else(|| self.question_tone.clone()),
            question_push_per_day: over.question_push_per_day.or(self.question_push_per_day),
            question_push_cooldown_minutes: over
                .question_push_cooldown_minutes
                .or(self.question_push_cooldown_minutes),
        }
    }
}

/// 读出来给人用的那一份：每一项都已经落到具体值（不再有空档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResolvedAppearance {
    pub jump_to_end_on_latest: bool,
    /// 作者选过的口径；`None` = 没选过（界面用作品语言的默认口径顶上）。
    pub word_count_caliber: Option<WordCaliber>,
    /// 引号用哪一套（没选过就是弯引号）。
    pub quote_style: QuoteStyle,
    /// 每日码字目标；`None` = 没设目标（界面就不显示进度条，只显示今日写了多少）。
    pub daily_goal: Option<i64>,
    /// 作者选过的命名规则；`None` = 没选过（界面按作品类型显示默认那一档）。
    pub naming: Option<NamingStyle>,
    /// 章的号跨不跨卷数（没选过就是默认：跨卷延续）。
    pub chapter_numbering: ChapterNumbering,
    /// 正文字号 px / 行距百分比 / 字距百分比；`None` = 没改过（界面用自己那档默认）。
    ///
    /// 三项都是**显示层**：只改阅读观感，不进导出、不动正文（由调用方绑到 CSS 变量上）。
    pub editor_font_size: Option<i64>,
    pub editor_line_height: Option<i64>,
    pub editor_letter_spacing: Option<i64>,
    /// 叩问问话的语气（没改过就是温柔那档；`neutral` = 不加语气）。
    pub question_tone: QuestionTone,
    /// 写的时候最多主动问几次（0 = 不打扰）；只限"推"。
    pub question_push_per_day: i64,
    /// 两次主动问之间的冷却（分钟）。
    pub question_push_cooldown_minutes: i64,
}

impl Default for ResolvedAppearance {
    /// 默认值只有这一处——界面与核心都不许各写一份。
    fn default() -> Self {
        Self {
            jump_to_end_on_latest: true,
            word_count_caliber: None,
            quote_style: QuoteStyle::default(),
            daily_goal: None,
            naming: None,
            chapter_numbering: ChapterNumbering::default(),
            // 排版三项默认"没设过"：具体用多少 px / 多少倍是**界面**的事（核心不碰观感数值）
            editor_font_size: None,
            editor_line_height: None,
            editor_letter_spacing: None,
            // 叩问：默认温柔（贴心那档）、一天最多 3 次、两次之间隔 60 分钟
            // ——"主动开口"必须稀缺，写作最怕被打断
            question_tone: QuestionTone::default(),
            question_push_per_day: DEFAULT_PUSH_PER_DAY,
            question_push_cooldown_minutes: DEFAULT_PUSH_COOLDOWN_MINUTES,
        }
    }
}

impl Store {
    /// 读偏好：**书的覆盖 → 全局 → 默认**。
    ///
    /// `work_id` 给 `Some` 会先看这本书有没有单独设过；给 `None` 只看全局。
    pub fn appearance(&self, work_id: Option<i64>) -> Result<ResolvedAppearance> {
        let global = self.read_appearance(None)?;
        let merged = match work_id {
            Some(id) => global.overridden_by(&self.read_appearance(Some(id))?),
            None => global,
        };
        let defaults = ResolvedAppearance::default();
        Ok(ResolvedAppearance {
            jump_to_end_on_latest: merged
                .jump_to_end_on_latest
                .unwrap_or(defaults.jump_to_end_on_latest),
            // 认不出来的代码**当没设过**（跟坏 JSON 同一条规矩），界面回语言默认
            word_count_caliber: merged
                .word_count_caliber
                .as_deref()
                .and_then(WordCaliber::parse),
            // 同上：认不出来的引号风格当没设过，回默认那一套
            quote_style: merged
                .quote_style
                .as_deref()
                .and_then(QuoteStyle::parse)
                .unwrap_or_default(),
            // 目标：非正数当没设过（0 = 作者把目标清掉了）；上限挡一下手滑输入的离谱数
            daily_goal: merged.daily_goal.filter(|v| *v > 0).map(|v| v.min(MAX_DAILY_GOAL)),
            // 认不出来的代码当没设过（与口径 / 引号同一条规矩），界面回"按作品类型"
            naming: merged.naming.as_deref().and_then(NamingStyle::parse),
            // 同上：认不出来当没设过 → 回默认（跨卷延续）
            chapter_numbering: merged
                .chapter_numbering
                .as_deref()
                .and_then(ChapterNumbering::parse)
                .unwrap_or_default(),
            // 排版三项：坏数据 / 极端值都夹进可读范围（只影响观感，不值得为它报错）
            editor_font_size: merged
                .editor_font_size
                .and_then(|value| typography_value(value, FONT_SIZE_RANGE)),
            editor_line_height: merged
                .editor_line_height
                .and_then(|value| typography_value(value, LINE_HEIGHT_RANGE)),
            editor_letter_spacing: merged
                .editor_letter_spacing
                .and_then(|value| typography_value(value, LETTER_SPACING_RANGE)),
            // 语气：认不出来的代码当没设过（与口径 / 引号同一条规矩），回"温柔"
            question_tone: merged
                .question_tone
                .as_deref()
                .and_then(|code| QuestionTone::parse(code).ok())
                .unwrap_or_default(),
            // 配额与冷却：手滑输入离谱数就夹进可接受范围（0 是正经取值 = 不打扰）
            question_push_per_day: clamp_count(
                merged.question_push_per_day,
                DEFAULT_PUSH_PER_DAY,
                MAX_PUSH_PER_DAY,
            ),
            question_push_cooldown_minutes: clamp_count(
                merged.question_push_cooldown_minutes,
                DEFAULT_PUSH_COOLDOWN_MINUTES,
                MAX_PUSH_COOLDOWN_MINUTES,
            ),
        })
    }

    /// **落定后的字数口径**：作者选过就听作者的，没选过跟作品语言的默认。
    ///
    /// 这条规则只写在这里——壳与界面都别自己拼（界面尤其不该抄一份"语言 → 口径"对照表，
    /// 两份迟早走偏）。改作品语言后若作者从没选过，这里自然就跟着新语言走。
    pub fn word_caliber(&self, work_id: i64) -> Result<WordCaliber> {
        if let Some(chosen) = self.appearance(Some(work_id))?.word_count_caliber {
            return Ok(chosen);
        }
        Ok(self.get_work(work_id)?.language.default_caliber())
    }

    /// **落定后的命名规则**：作者选过就听作者的，没选过跟作品类型的默认。
    ///
    /// 与 [`Store::word_caliber`] 同一条思路：规则只写在这里，壳与界面都不许自己抄一份
    /// "类型 → 规则"对照表（两份迟早走偏）。
    pub fn naming_style(&self, work_id: i64) -> Result<NamingStyle> {
        if let Some(chosen) = self.appearance(Some(work_id))?.naming {
            return Ok(chosen);
        }
        Ok(self.get_work(work_id)?.kind.default_naming())
    }

    /// **落定后的章节编号方式**：作者选过就听作者的，没选过是默认（跨卷延续）。
    ///
    /// 与 [`Store::naming_style`] 同一处出口：渲染标题的地方都从这里取口径，
    /// 界面与壳都不许自己抄一份"怎么数"（两份迟早走偏）。
    pub fn chapter_numbering(&self, work_id: i64) -> Result<ChapterNumbering> {
        Ok(self.appearance(Some(work_id))?.chapter_numbering)
    }

    /// 写偏好（**稀疏合并**）：只覆盖传进来的项，没传的保持原样。
    ///
    /// `work_id` 给 `Some` 就是"这本书单独设"，给 `None` 就是全局。
    pub fn set_appearance(&mut self, work_id: Option<i64>, patch: &Appearance) -> Result<()> {
        let mut stored = self.read_appearance(work_id)?;
        if let Some(value) = patch.jump_to_end_on_latest {
            stored.jump_to_end_on_latest = Some(value);
        }
        if let Some(value) = patch.word_count_caliber.as_deref() {
            // 只认三种稳定代码：写进来一个不认识的，等于给界面埋一个"未知口径"
            let parsed = WordCaliber::parse(value).ok_or_else(|| {
                crate::error::Error::invalid_with(
                    crate::error::codes::UNKNOWN_WORD_CALIBER,
                    [("value", value.to_string())],
                )
            })?;
            stored.word_count_caliber = Some(parsed.as_str().to_string());
        }
        if let Some(value) = patch.quote_style.as_deref() {
            // 同一条纪律：只认两个稳定代码，写进来不认识的等于给界面埋"未知风格"
            let parsed = QuoteStyle::parse(value).ok_or_else(|| {
                crate::error::Error::invalid_with(
                    crate::error::codes::UNKNOWN_QUOTE_STYLE,
                    [("value", value.to_string())],
                )
            })?;
            stored.quote_style = Some(parsed.as_str().to_string());
        }
        if let Some(value) = patch.naming.as_deref() {
            // `"auto"` = **清掉这一层**（回到"按作品类型"）——与每日目标用 0 清掉同一条思路：
            // 界面要能表达"这一本不要单独设了"，而 `None`（不传）表达的是"这项不改"。
            if value == "auto" {
                stored.naming = None;
            } else {
            // 同一条纪律：只认四个稳定代码，写进来不认识的等于给界面埋"未知规则"
            let parsed = NamingStyle::parse(value).ok_or_else(|| {
                crate::error::Error::invalid_with(
                    crate::error::codes::UNKNOWN_NAMING_STYLE,
                    [("value", value.to_string())],
                )
            })?;
            stored.naming = Some(parsed.as_str().to_string());
            }
        }
        if let Some(value) = patch.daily_goal {
            // 0 / 负数 = **把目标清掉**（界面上的"不设目标"就是发一个 0 过来）；
            // 超大的值夹到上限，免得手滑多打几个零后进度条永远不动
            stored.daily_goal = (value > 0).then(|| value.min(MAX_DAILY_GOAL));
        }
        // 正文排版三项：**只写传进来的**（`None` = 这项不改）；传 ≤0 = 清掉回默认；
        // 超出可读范围的夹住——它们是纯观感，写坏一个数不该把版面炸了，更不该报错挡人
        if let Some(value) = patch.editor_font_size {
            stored.editor_font_size = typography_value(value, FONT_SIZE_RANGE);
        }
        if let Some(value) = patch.editor_line_height {
            stored.editor_line_height = typography_value(value, LINE_HEIGHT_RANGE);
        }
        if let Some(value) = patch.editor_letter_spacing {
            stored.editor_letter_spacing = typography_value(value, LETTER_SPACING_RANGE);
        }
        if let Some(value) = patch.chapter_numbering.as_deref() {
            // `"auto"` = 清掉这一层（回到默认：跨卷延续）——与命名规则同一条路
            if value == "auto" {
                stored.chapter_numbering = None;
            } else {
                let parsed = ChapterNumbering::parse(value).ok_or_else(|| {
                    crate::error::Error::invalid_with(
                        crate::error::codes::UNKNOWN_CHAPTER_NUMBERING,
                        [("value", value.to_string())],
                    )
                })?;
                stored.chapter_numbering = Some(parsed.as_str().to_string());
            }
        }
        if let Some(value) = patch.question_tone.as_deref() {
            // `"auto"` = 清掉这一层（回默认"温柔"）——与命名规则同一条路
            if value == "auto" {
                stored.question_tone = None;
            } else {
                let parsed = QuestionTone::parse(value).map_err(|_| {
                    crate::error::Error::invalid_with(
                        crate::error::codes::UNKNOWN_QUESTION_TONE,
                        [("value", value.to_string())],
                    )
                })?;
                stored.question_tone = Some(parsed.as_str().to_string());
            }
        }
        // 打扰度两项：**负数 = 清掉这一层**（回默认）；`0` 是正经取值（＝不打扰），
        // 所以不能像排版那样拿"≤0"当清掉——那会把"别打扰我"变成一个改不掉的空档
        if let Some(value) = patch.question_push_per_day {
            stored.question_push_per_day = (value >= 0).then(|| value.min(MAX_PUSH_PER_DAY));
        }
        if let Some(value) = patch.question_push_cooldown_minutes {
            stored.question_push_cooldown_minutes =
                (value >= 0).then(|| value.min(MAX_PUSH_COOLDOWN_MINUTES));
        }
        let tx = self.conn.transaction()?;
        write_appearance(&tx, work_id, &stored)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id.unwrap_or(0),
            "set_appearance",
            serde_json::json!({
                "jump_to_end_on_latest": patch.jump_to_end_on_latest,
                "word_count_caliber": patch.word_count_caliber,
                "quote_style": patch.quote_style,
                "daily_goal": patch.daily_goal,
                "naming": patch.naming,
                "chapter_numbering": patch.chapter_numbering,
                "question_tone": patch.question_tone,
                "question_push_per_day": patch.question_push_per_day,
                "question_push_cooldown_minutes": patch.question_push_cooldown_minutes,
                "editor_font_size": patch.editor_font_size,
                "editor_line_height": patch.editor_line_height,
                "editor_letter_spacing": patch.editor_letter_spacing,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 清掉一份偏好，**回到默认**（书的覆盖则是"回到继承全局"）。
    ///
    /// 与"传一个空 patch"不同：空 patch 是"这项不改"，这里是真的把记录抹掉。
    pub fn reset_appearance(&mut self, work_id: Option<i64>) -> Result<()> {
        let tx = self.conn.transaction()?;
        write_appearance(&tx, work_id, &Appearance::default())?;
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id.unwrap_or(0),
            "reset_appearance",
            serde_json::json!({}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 读一份（全局或某本书的覆盖）；读不出来当没设过。
    fn read_appearance(&self, work_id: Option<i64>) -> Result<Appearance> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![appearance_key(work_id)],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }
}

/// 写一份偏好；一项都没改过就把键删掉（不留空记录）。
///
/// 与 [`Store`](super::Store) 分开是为了让"从成稿导入"能在**它自己的那个事务里**把编号档一起落库
/// （新书还没有偏好记录，所以"稀疏合并"的那一半用不上）。
pub(super) fn write_appearance(
    conn: &rusqlite::Connection,
    work_id: Option<i64>,
    value: &Appearance,
) -> Result<()> {
    let key = appearance_key(work_id);
    if value.is_empty() {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        return Ok(());
    }
    let json = serde_json::to_string(value).unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
        params![key, json, now_millis()],
    )?;
    Ok(())
}

/// 偏好在 `settings` 里的键：全局一份，每本书可另存一份覆盖。
fn appearance_key(work_id: Option<i64>) -> String {
    match work_id {
        Some(id) => format!("work.{id}.appearance"),
        None => "appearance".to_string(),
    }
}
