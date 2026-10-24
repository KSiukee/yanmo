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
use crate::model::{
    BackgroundColor, BackgroundFit, BackgroundLayout, ChapterNumbering, NamingStyle, QuestionTone,
    RuleStyle, SideTab, Theme, DEFAULT_BG_OPACITY, DEFAULT_EYE_CARE, MAX_BG_OPACITY,
    MAX_EYE_CARE,
};
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
/// 用眼提醒的默认值与上限：**宽阈值**——它只在"作者要停下来"的时刻说一句、
/// 绝不打断写作，所以默认开着；嫌吵一键关掉。
const DEFAULT_BREAK_MINUTES: i64 = 45;
const DEFAULT_BREAK_WORDS: i64 = 1500;
/// 手滑兜底的上限：连着写 3 小时 / 一天 2 万字，已远超正常写作，再往上没有意义。
const MAX_BREAK_MINUTES: i64 = 180;
const MAX_BREAK_WORDS: i64 = 20_000;

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
    /// 正文底下那层稿纸横线用哪种线（`RuleStyle` 的稳定码：`none` / `solid` / `dashed`）。
    /// `None` = 没改过 → **不显示**（与研墨一直以来的样子一致），由读侧落定。
    ///
    /// 一格横线多高是**排版算出来的**（字号 × 行距，见界面 `editor/rule.ts`），核心不存那个
    /// 高度——存一份就会腐烂（作者一改行距，这边还记着上一次）。纯显示层：不进导出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_rule_style: Option<String>,
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
    /// 右侧第二栏现在露哪一块（`SideTab` 的稳定码：`flow` / `creator`）。
    ///
    /// "记住上次"这一类偏好：它只决定**那一栏先露哪一块**，不影响任何数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aside_tab: Option<String>,
    /// 界面配色（`Theme` 的稳定码：`light` / `dark` / `system`）。
    ///
    /// `None` = 没改过 → **浅色**（与研墨一直以来的样子一致）。
    ///
    /// ★ 本项**只走全局**、不参与每书覆盖（跟眼睛与房间光线有关，跟哪本书无关）——
    /// 所以它不出现在 `Appearance::overridden_by` 里，写也只在全局那一层落库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// 用眼提醒开着没有（`None` = 没改过 → **开**：它只在停下点说一句、不打断写作，
    /// 所以宽阈值默认开着；嫌吵一键关掉——关掉就是一声不响）。
    ///
    /// ★ 与配色同一条口径：本项**只走全局**、不参与每书覆盖（"今天写了多少"与"坐了多久"
    /// 跟哪本书无关）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_reminder: Option<bool>,
    /// 连着写满多少分钟提醒一次（`None` = 没改过 → 45）。
    ///
    /// 也是**只走全局**：见上一条。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_minutes: Option<i64>,
    /// 今天写到多少字提醒一次（`None` = 没改过 → 1500）。
    ///
    /// 同样**只走全局**。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_words: Option<i64>,
    /// 护眼滤色的幅度（百分比；`0` = 关，`None` = 没改过 → 也是关）。
    ///
    /// 这一层是**整窗的一层暖黄膜**（只染研墨自己的窗口）：它压在什么底上（浅色稿纸还是
    /// 深色暖黑）决定了"最深能到多少"，那件事只有界面当场知道——所以库里只存作者调的
    /// 幅度，深浅合计出来的上限由界面算（见 `model::eye_care` 的文件头）。
    ///
    /// `0` 是**正经取值**（＝完全不染，一个像素都不动），所以不能拿它当"清掉这一层"
    /// （与图片不透明度同一个坑）；越界夹进 0~100，不报错——它是纯观感。
    ///
    /// ★ 与配色、用眼提醒同一条口径：本项**只走全局**、不参与每书覆盖
    /// （跟眼睛与房间光线有关，跟哪本书无关）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eye_care: Option<i64>,
    /// 正文底色（`BackgroundColor` 的稳定码：`paper` / `cream` / `green` / `gray`）。
    ///
    /// `None` = 没改过 → **纸白**（与研墨一直以来的编辑区底色一致）。
    /// 每书覆盖走同一条路（机制先留着，界面第一版只暴露全局）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_bg_color: Option<String>,
    /// 正文底纹（`BackgroundLayout` 的稳定码：`none` / `grid` / `dots` / `grain`）。
    ///
    /// `None` = 没改过 → **不铺**。一格的边长由界面按"字号 × 行距"算出来，核心不存
    /// （存一份就会腐烂：作者一改行距，这边还记着上一次——与稿纸横线同一条理由）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_bg_layout: Option<String>,
    /// 背景图**在数据目录里的文件名**（`None` = 没设过）。
    ///
    /// ★ **不存路径**：图由壳在"作者选图"那一步复制进数据目录（跟着稿子走，
    /// 备份与搬家都带上它），库里只留一个名字。界面拿不到路径，也就拼不出目录外的东西。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_bg_image: Option<String>,
    /// 背景图铺法（`BackgroundFit` 的稳定码：`cover` / `contain` / `tile`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_bg_fit: Option<String>,
    /// 背景图那一层的不透明度（百分比；`None` = 没改过 → 35）。
    ///
    /// `0` 是正经取值（＝图片层完全看不见，但图还留着）；越界由写侧**夹进 0~100**，
    /// 与排版三项同一条路——它是纯观感，写坏一个数不该报错挡人。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_bg_opacity: Option<i64>,
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

/// 用眼提醒那两个阈值的校验：**≤0 与超过上限都一律拒**。
///
/// 为什么不照排版那样夹住：这两个数要拿去跟"连着写了多久 / 今天写了多少"比，一个 0
/// 或负数会让它**每时每刻**都够阈值——那正好把"安静"这件事整个反转了。所以宁可当场
/// 说不收（界面把码渲染成一句人话），也不悄悄替作者改成另一个数。
fn break_threshold(value: i64, max: i64, code: &'static str) -> Result<i64> {
    if (1..=max).contains(&value) {
        Ok(value)
    } else {
        Err(crate::error::Error::invalid_with(
            code,
            [
                ("value", value.to_string()),
                ("max", max.to_string()),
            ],
        ))
    }
}

/// `"auto"` = **清掉这一层**（回默认 / 回继承）；否则交给 `parse` 认码，认不出当场拒。
///
/// 与命名规则、配色、稿纸线型走的是同一条路（界面要能表达"这一项不要单独设了"，
/// 而"不传"表达的是"这项不改"——两者不是一回事）。
fn clear_or(value: &str, parse: impl Fn(&str) -> Result<&'static str>) -> Result<Option<String>> {
    if value == "auto" {
        return Ok(None);
    }
    Ok(Some(parse(value)?.to_string()))
}

/// 把库里那一格**当文件名**读：只认"最后一段名字"，别的一律当没设过。
///
/// 为什么这么严：这跟"界面拿不到路径"是同一件事的两面——界面上递过来的是文件名，
/// 但库里那一格可能是老版本、手改过的库、或将来某个调用点写坏的。**在这里把目录成分
/// 一刀切掉**，壳那边再拼数据目录，就没有任何一条路能让它指到别处去。
///
/// 坏值（空、`.`/`..`、带路径分隔符、带盘符的绝对路径）一律 `None` = 没设过，
/// 界面回"没有背景图"——**不报错**：作者看到的应当是"图没了，回纯色"，
/// 而不是一个把他挡在外面的错误框。
pub(crate) fn image_name(raw: &str) -> Option<String> {
    let name = raw.trim().rsplit(['/', '\\']).next().unwrap_or("");
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    // 去掉 Windows 上不允许出现在文件名里的字符（冒号一并去掉：它能拼出盘符）
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    (!cleaned.is_empty()).then_some(cleaned)
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
            && self.editor_rule_style.is_none()
            && self.question_tone.is_none()
            && self.question_push_per_day.is_none()
            && self.question_push_cooldown_minutes.is_none()
            && self.aside_tab.is_none()
            && self.theme.is_none()
            && self.break_reminder.is_none()
            && self.break_minutes.is_none()
            && self.break_words.is_none()
            && self.eye_care.is_none()
            && self.editor_bg_color.is_none()
            && self.editor_bg_layout.is_none()
            && self.editor_bg_image.is_none()
            && self.editor_bg_fit.is_none()
            && self.editor_bg_opacity.is_none()
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
            // 稿纸线型：每书覆盖也走同一条路（第一版界面只暴露全局，机制先留着）
            editor_rule_style: over
                .editor_rule_style
                .clone()
                .or_else(|| self.editor_rule_style.clone()),
            question_tone: over.question_tone.clone().or_else(|| self.question_tone.clone()),
            question_push_per_day: over.question_push_per_day.or(self.question_push_per_day),
            question_push_cooldown_minutes: over
                .question_push_cooldown_minutes
                .or(self.question_push_cooldown_minutes),
            aside_tab: over.aside_tab.clone().or_else(|| self.aside_tab.clone()),
            // 配色：**只认全局那一层**（跟眼睛与房间光线有关，跟哪本书无关）。
            // 于是 `over`（书的覆盖）里的 theme 在这里被**有意丢掉**——就算哪本书的记录里
            // 真写了它，也翻不动界面；`set_appearance` 那边也不再往每书记录里写（见那里）。
            theme: self.theme.clone(),
            // 用眼提醒三项：与配色同一条口径——**只认全局那一层**。
            // `over`（书的覆盖）里的这三个字段在这里同样被有意丢掉，写那边也不落每书记录。
            break_reminder: self.break_reminder,
            break_minutes: self.break_minutes,
            break_words: self.break_words,
            // 护眼滤色：与配色同一条口径——**只认全局那一层**（`over` 里那份被有意丢掉）。
            eye_care: self.eye_care,
            // 正文背景那一族：每书覆盖也走同一条路（第一版界面只暴露全局，机制先留着）
            editor_bg_color: over.editor_bg_color.clone().or_else(|| self.editor_bg_color.clone()),
            editor_bg_layout: over
                .editor_bg_layout
                .clone()
                .or_else(|| self.editor_bg_layout.clone()),
            // 图片也是"每书可换一张"：跟着书走
            editor_bg_image: over.editor_bg_image.clone().or_else(|| self.editor_bg_image.clone()),
            editor_bg_fit: over.editor_bg_fit.clone().or_else(|| self.editor_bg_fit.clone()),
            editor_bg_opacity: over.editor_bg_opacity.or(self.editor_bg_opacity),
        }
    }
}

/// 读出来给人用的那一份：每一项都已经落到具体值（不再有空档）。
///
/// ⚠️ 不能是 `Copy`：背景图那一格是一个 **`Option<String>`**（文件名），
/// 也就是说这一份里第一次有了"要搬走的内存"。它本来就是**读出来看一眼**的东西，
/// 谁要留就 `clone()`——这比把文件名偷偷丢掉强。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// 稿纸横线的线型（没改过就是不显示）。
    pub editor_rule_style: RuleStyle,
    /// 叩问问话的语气（没改过就是温柔那档；`neutral` = 不加语气）。
    pub question_tone: QuestionTone,
    /// 写的时候最多主动问几次（0 = 不打扰）；只限"推"。
    pub question_push_per_day: i64,
    /// 两次主动问之间的冷却（分钟）。
    pub question_push_cooldown_minutes: i64,
    /// 右侧第二栏先露哪一块——"记住上次"的就是它。
    pub aside_tab: SideTab,
    /// 界面配色（没改过就是浅色）。**只走全局**这一层。
    pub theme: Theme,
    /// 用眼提醒：开关 / 连着写多少分钟 / 今天写到多少字（**只走全局**这一层）。
    pub break_reminder: bool,
    pub break_minutes: i64,
    pub break_words: i64,
    /// 护眼滤色的幅度（0~100；**0 = 关**，没改过也是 0）。**只走全局**这一层。
    ///
    /// 这里存的是作者调的幅度，不是那层膜的最终深度——深浅两套配色合计出来的上限由界面算
    /// （见 `model::eye_care` 的文件头）。
    pub eye_care: i64,
    /// 正文底色（没改过就是纸白）。
    pub editor_bg_color: BackgroundColor,
    /// 正文底纹（没改过就是不铺）。
    pub editor_bg_layout: BackgroundLayout,
    /// 背景图在数据目录里的文件名；`None` = 没设过（也**不带路径**）。
    pub editor_bg_image: Option<String>,
    /// 背景图铺法（没改过就是"铺满"）。
    pub editor_bg_fit: BackgroundFit,
    /// 背景图那一层的不透明度（0~100；没改过就是 35）。
    pub editor_bg_opacity: i64,
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
            editor_rule_style: RuleStyle::default(),
            // 叩问：默认温柔（贴心那档）、一天最多 3 次、两次之间隔 60 分钟
            // ——"主动开口"必须稀缺，写作最怕被打断
            question_tone: QuestionTone::default(),
            question_push_per_day: DEFAULT_PUSH_PER_DAY,
            question_push_cooldown_minutes: DEFAULT_PUSH_COOLDOWN_MINUTES,
            aside_tab: SideTab::default(),
            // 配色：默认浅色（不给没提过这件事的人换底子）
            theme: Theme::default(),
            // 用眼提醒：宽阈值默认开着（只在停下点说一句、不打断写作；嫌吵一键关）
            break_reminder: true,
            break_minutes: DEFAULT_BREAK_MINUTES,
            break_words: DEFAULT_BREAK_WORDS,
            // 护眼滤色：默认**关**（0）——没提过这件事的人屏幕上与一直以来的样子一样
            eye_care: DEFAULT_EYE_CARE,
            // 正文背景：默认与研墨一直以来的样子**一模一样**（纸白、不铺、没有图）
            editor_bg_color: BackgroundColor::default(),
            editor_bg_layout: BackgroundLayout::default(),
            editor_bg_image: None,
            editor_bg_fit: BackgroundFit::default(),
            editor_bg_opacity: DEFAULT_BG_OPACITY,
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
            // 线型：认不出来的码当没设过（与口径 / 引号 / 配色同一条规矩），回"不显示"
            editor_rule_style: merged
                .editor_rule_style
                .as_deref()
                .and_then(|code| RuleStyle::parse(code).ok())
                .unwrap_or_default(),
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
            // 第二栏：认不出来的码当没设过（与口径 / 引号 / 语气同一条规矩），回"叩问"
            aside_tab: merged
                .aside_tab
                .as_deref()
                .and_then(|code| SideTab::parse(code).ok())
                .unwrap_or_default(),
            // 配色：同上，认不出来的当没设过，回浅色
            theme: merged
                .theme
                .as_deref()
                .and_then(|code| Theme::parse(code).ok())
                .unwrap_or_default(),
            // 用眼提醒：开关没有可坏的取值（坏 JSON 那条规矩已经把它兜住）；
            // 两个阈值坏成 ≤0 或超出可接受范围时**当没设过**、回默认那一档——
            // 读侧不报错（跟坏口径 / 坏配色同一条规矩），写侧才当场拒（见 `break_threshold`）。
            break_reminder: merged.break_reminder.unwrap_or(defaults.break_reminder),
            break_minutes: merged
                .break_minutes
                .filter(|value| (1..=MAX_BREAK_MINUTES).contains(value))
                .unwrap_or(defaults.break_minutes),
            break_words: merged
                .break_words
                .filter(|value| (1..=MAX_BREAK_WORDS).contains(value))
                .unwrap_or(defaults.break_words),
            // 护眼滤色：越界的坏值**当没设过**、回默认（不报错，与排版那三项同一条规矩）；
            // `0` 是正经取值（＝关），所以过滤器是 `0..=MAX`，不是"非正数当没设过"
            eye_care: merged
                .eye_care
                .filter(|value| (0..=MAX_EYE_CARE).contains(value))
                .unwrap_or(defaults.eye_care),
            // 正文背景三层：认不出来的码**当没设过**（与口径 / 引号 / 线型同一条规矩），
            // 回默认那一档；不透明度夹进 0~100（它是纯观感，不值得为它报错挡人）
            editor_bg_color: merged
                .editor_bg_color
                .as_deref()
                .and_then(|code| BackgroundColor::parse(code).ok())
                .unwrap_or_default(),
            editor_bg_layout: merged
                .editor_bg_layout
                .as_deref()
                .and_then(|code| BackgroundLayout::parse(code).ok())
                .unwrap_or_default(),
            // 图片：只认一个**文件名**（去掉任何目录成分；剩下的交给壳在数据目录里找）
            editor_bg_image: merged.editor_bg_image.as_deref().and_then(image_name),
            editor_bg_fit: merged
                .editor_bg_fit
                .as_deref()
                .and_then(|code| BackgroundFit::parse(code).ok())
                .unwrap_or_default(),
            editor_bg_opacity: merged
                .editor_bg_opacity
                .map(|value| value.clamp(0, MAX_BG_OPACITY))
                .unwrap_or(defaults.editor_bg_opacity),
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
        // 稿纸线型：`"auto"` = 清掉这一层（回默认"不显示"），与命名 / 语气 / 配色同一条路；
        // 别的取值只认那三个稳定码（写进来不认识的，等于给界面埋一个"不知道该画哪种线"的空档）
        if let Some(value) = patch.editor_rule_style.as_deref() {
            stored.editor_rule_style = if value == "auto" {
                None
            } else {
                Some(RuleStyle::parse(value)?.as_str().to_string())
            };
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
        if let Some(value) = patch.aside_tab.as_deref() {
            // `"auto"` = 清掉这一层（回默认"叩问"）——与命名规则、语气同一条路；
            // 别的取值只认那两个分区码（写进来不认识的会变成一个"不知道露哪块"的空档）
            if value == "auto" {
                stored.aside_tab = None;
            } else {
                let parsed = SideTab::parse(value)?;
                stored.aside_tab = Some(parsed.as_str().to_string());
            }
        }
        // 配色：**只落全局那一层**（跟眼睛与房间光线有关，跟哪本书无关）。
        // 每书传上来的 theme 有意不写——读那边的合并本来就不认它，写下去只会在每书记录里
        // 留一个永远没人读的死键。`"auto"` = 清掉这一层（回默认浅色），与命名 / 语气同一条路。
        if let Some(value) = patch.theme.as_deref() {
            if work_id.is_none() {
                if value == "auto" {
                    stored.theme = None;
                } else {
                    let parsed = Theme::parse(value)?;
                    stored.theme = Some(parsed.as_str().to_string());
                }
            }
        }
        // 用眼提醒三项：与配色同一条口径——**只落全局那一层**（每书传上来的有意不写，
        // 读那边的合并本来就不认它）。两个阈值 ≤0 或超出上限当场拒（见 `break_threshold`）。
        if work_id.is_none() {
            if let Some(value) = patch.break_reminder {
                stored.break_reminder = Some(value);
            }
            if let Some(value) = patch.break_minutes {
                stored.break_minutes = Some(break_threshold(
                    value,
                    MAX_BREAK_MINUTES,
                    crate::error::codes::BREAK_MINUTES_OUT_OF_RANGE,
                )?);
            }
            if let Some(value) = patch.break_words {
                stored.break_words = Some(break_threshold(
                    value,
                    MAX_BREAK_WORDS,
                    crate::error::codes::BREAK_WORDS_OUT_OF_RANGE,
                )?);
            }
        }
        // 护眼滤色：与配色 / 用眼提醒同一条口径——**只落全局那一层**（每书传上来的有意不写，
        // 读那边的合并本来就不认它）。`0` 是正经取值（＝关，一个像素都不染），所以不能拿
        // "非正数"当清掉；负数才是清掉这一层（与图片不透明度同一个坑）、越界夹住不报错。
        if work_id.is_none() {
            if let Some(value) = patch.eye_care {
                stored.eye_care = (value >= 0).then(|| value.min(MAX_EYE_CARE));
            }
        }
        // ── 正文背景那一族（三层各自独立，与排版同一条路）──────────────────
        // 底色 / 底纹 / 铺法：`"auto"` = 清掉这一层（回默认），别的取值只认那几个稳定码
        // （写进来不认识的，等于给界面埋一个"不知道该画什么"的空档）
        if let Some(value) = patch.editor_bg_color.as_deref() {
            stored.editor_bg_color = clear_or(value, |code| BackgroundColor::parse(code).map(|v| v.as_str()))?;
        }
        if let Some(value) = patch.editor_bg_layout.as_deref() {
            stored.editor_bg_layout =
                clear_or(value, |code| BackgroundLayout::parse(code).map(|v| v.as_str()))?;
        }
        if let Some(value) = patch.editor_bg_fit.as_deref() {
            stored.editor_bg_fit = clear_or(value, |code| BackgroundFit::parse(code).map(|v| v.as_str()))?;
        }
        // 不透明度：负数 = 清掉这一层（回默认 35）；`0` 是正经取值（＝图还留着但看不见），
        // 所以不能像排版那样拿"≤0"当清掉——那会把"先藏起来"变成一个改不掉的空档
        // （与叩问那两项同一个坑）。越界夹进 0~100。
        if let Some(value) = patch.editor_bg_opacity {
            stored.editor_bg_opacity = (value >= 0).then(|| value.min(MAX_BG_OPACITY));
        }
        // 图片只走**专门的那一条路**（`set_background_image`：壳把文件复制好再叫它），
        // 这里有意不收 `editor_bg_image`：界面手里没有文件名，谁都不该凭空写一个进来。
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
                "aside_tab": patch.aside_tab,
                "theme": patch.theme,
                "break_reminder": patch.break_reminder,
                "break_minutes": patch.break_minutes,
                "break_words": patch.break_words,
                "eye_care": patch.eye_care,
                "editor_font_size": patch.editor_font_size,
                "editor_line_height": patch.editor_line_height,
                "editor_letter_spacing": patch.editor_letter_spacing,
                "editor_rule_style": patch.editor_rule_style,
                "editor_bg_color": patch.editor_bg_color,
                "editor_bg_layout": patch.editor_bg_layout,
                "editor_bg_fit": patch.editor_bg_fit,
                "editor_bg_opacity": patch.editor_bg_opacity,
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 记下"背景图换成了哪一个文件"（**由壳在复制完之后调用**）。
    ///
    /// 单独一条路、而不是混进 [`Store::set_appearance`]：图片这一步**先有文件、后有记录**
    /// （壳把图写进数据目录成功之后才叫这里），与别的偏好"只动一个数"不是一回事。
    /// 传 `None` = 清掉这一层。图多大由壳当场告诉界面（那是那一次动作的回执，不是偏好）。
    pub fn set_background_image(&mut self, work_id: Option<i64>, name: Option<&str>) -> Result<()> {
        let mut stored = self.read_appearance(work_id)?;
        stored.editor_bg_image = name.and_then(image_name);
        let tx = self.conn.transaction()?;
        write_appearance(&tx, work_id, &stored)?;
        Self::record_in(
            &self.device_id,
            &tx,
            "settings",
            work_id.unwrap_or(0),
            "set_background_image",
            serde_json::json!({ "name": stored.editor_bg_image }),
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
