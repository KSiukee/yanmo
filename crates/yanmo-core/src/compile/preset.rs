//! 编译预设：一份原稿 → 一种"拿去用"的成品。
//!
//! 预设是**稳定代码**（界面按码取名字与说明，核心不给句子），参数跟着预设走；
//! 产物一律落在导出目录下**各自的子目录**里——换一种预设编译不会把上一种的产物冲掉
//! （子目录名语言无关：它会留在作者的磁盘上）。

use serde::Serialize;

use crate::error::{codes, Error, Result};

/// 一次编译要产出哪种成品。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// 投稿版：单个 `docx`（正文 + 大纲）——给编辑看的那一份
    SubmissionDocx,
    /// 单篇投稿版：一份短稿一个 `docx`（**书名页 + 正文**，不附大纲）
    ArticleDocx,
    /// 分章纯文本：一章一个文件，结构用目录表示（最容易被人接手）
    ChaptersTxt,
    /// 合并纯文本：全书拼成一个文件（通读、贴到别处用）
    MergedTxt,
}

impl Preset {
    /// 全部预设（顺序就是界面上的顺序）。
    pub const ALL: &'static [Preset] = &[
        Preset::SubmissionDocx,
        Preset::ArticleDocx,
        Preset::ChaptersTxt,
        Preset::MergedTxt,
    ];

    /// 稳定代码（也用作产物子目录名）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubmissionDocx => "submission_docx",
            Self::ArticleDocx => "article_docx",
            Self::ChaptersTxt => "chapters_txt",
            Self::MergedTxt => "merged_txt",
        }
    }

    /// 从稳定代码解析；认不出报 `value.unknown_compile_preset`。
    pub fn parse(text: &str) -> Result<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|preset| preset.as_str() == text)
            .ok_or_else(|| {
                Error::invalid_with(codes::UNKNOWN_COMPILE_PRESET, [("value", text.to_string())])
            })
    }

    /// 产物子目录（语言无关：它会留在作者的磁盘上）。
    pub const fn folder(self) -> &'static str {
        match self {
            Self::SubmissionDocx => "submission",
            Self::ArticleDocx => "article",
            Self::ChaptersTxt => "chapters",
            Self::MergedTxt => "merged",
        }
    }

    /// 这一种预设要不要在正文上设字数上限。
    ///
    /// 长篇投稿版要（编辑先看开头那几万字）；**单篇投稿版不要**——
    /// 短篇本来就短，一刀切掉半篇反而是把作者的稿子弄残。
    pub const fn caps_body(self) -> bool {
        matches!(self, Self::SubmissionDocx)
    }

    /// 这一档产出的文件扩展名。
    ///
    /// 放在核心而不是壳里：**清理旧产物按扩展名认**（改了名 / 删了篇之后的孤儿），
    /// 认错了就会把上一份产物留在文件夹里，或者把别档的产物删掉。
    pub const fn extension(self) -> &'static str {
        match self {
            Self::SubmissionDocx | Self::ArticleDocx => "docx",
            Self::ChaptersTxt | Self::MergedTxt => "txt",
        }
    }
}

/// 一次编译的参数（界面给，核心不猜）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileOptions {
    /// 投稿版正文上限（逐字口径，含标点）；`None` = 不限。
    ///
    /// 按**段落**累加，不会把一个自然段切成两半；一章都没装下时至少装下第一段。
    pub body_limit: Option<i64>,
    /// 投稿版要不要附大纲。
    pub with_outline: bool,
}

impl CompileOptions {
    /// 投稿版的默认口径：**前 3 万字 + 大纲**（起点编辑那边的常见要求）。
    pub const DEFAULT_BODY_LIMIT: i64 = 30_000;

    /// 默认参数：投稿版按 3 万字上限、附大纲；其余预设不限、也不附大纲。
    pub fn defaults_for(preset: Preset) -> Self {
        Self {
            body_limit: preset.caps_body().then_some(Self::DEFAULT_BODY_LIMIT),
            with_outline: preset.caps_body(),
        }
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self::defaults_for(Preset::SubmissionDocx)
    }
}
