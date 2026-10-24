//! **分类归属那一格的规矩**：体裁怎么归一、标签怎么从一列文本读出来、哪些写法要当场拒。
//!
//! 它服务两件事：① 作品表单上"体裁 / 标签"这一组；② 短篇集里「按主题」那一档排序
//! （`store::collection_row` 把它读进 `ArticleEntry`）。
//!
//! # 三条口径
//!
//! 1. **存的是作者的原话，只是收拾干净**——体裁去掉首尾空白（它就是**一个**名字，
//!    前后带空格一定是手滑）；标签按行读、去首尾空白、**保留作者写的先后**（先写的更像主题）。
//!    不去改标点、不动中间的空白：`硬 科幻` 与 `硬科幻` 是两个标签，那是作者的写法。
//! 2. **重复的合并，空的丢掉**（同一本书上同一个标签写两遍没有意义；空行是笔误）。
//! 3. **超限就拒，绝不悄悄截断**——把作者写的第 13 个标签悄悄吃掉，比报一句
//!    "最多 12 个"更糟：他看不到自己写的东西，还以为是软件丢了。
//!
//! # 为什么不留"标签里能不能有逗号"这种问题
//!
//! 一行一个，**换行是标签里唯一不许出现的东西**（它是分隔符本身）。
//! 逗号、空格、斜杠都随便写——所以归一之后按行 join 回去，读到的一定就是作者写的那些。

use crate::error::{codes, Error, Result};

/// 一本书的分类归属：**体裁 + 标签**（"分类维度"那一摊里人填的两格）。
///
/// 读回来的形状就是库里的形状（体裁一行、标签一列多行）——缓存与回执都用它，
/// 两处**同一个类型**，免得一边改了另一边悄悄不同步（与 `word_caliber` 的 ack 同一条纪律）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkClassify {
    /// 体裁（`''` = 没填过）
    pub genre: String,
    /// 标签（一行一个，`''` = 一个都没填）——要列表就用 [`parse_tags`]
    pub tags: String,
}

/// 一个标签最长多少个字符（够写"赛博朋克+硬科幻"这种长标签，又不至于把一段话当标签）。
pub const MAX_TAG_CHARS: usize = 24;
/// 一本书最多几个标签。
///
/// 上限的用意不是"省地方"，是**这一栏别长成第二个大纲**：超过一打，
/// 「按主题」排出来的组就没法看了，作者自己也记不住。到点如实说，不悄悄丢。
pub const MAX_TAGS: usize = 12;
/// 体裁最长多少个字符（一个名字的长度，比标签宽一点）。
pub const MAX_GENRE_CHARS: usize = 32;

/// 把一列文本读成标签：**按行读、去空白、去重、保留先后**。
///
/// 认的换行：`\n` / `\r\n` / `\r`（三个都算——作者可能从别处粘来一段带 CRLF 的东西）。
/// 这是**读**那一半：库里那一列怎么写的，都解得出来；坏值不报错，坏值只是"多一个空行"。
pub fn parse_tags(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in raw.split(['\n', '\r']) {
        let tag = line.trim();
        if tag.is_empty() || out.iter().any(|kept| kept == tag) {
            continue;
        }
        out.push(tag.to_string());
    }
    out
}

/// 把标签写回一列文本：一行一个（与 [`parse_tags`] 是**一对**——写进去的一定读得回来）。
pub fn join_tags(tags: &[String]) -> String {
    tags.join("\n")
}

/// 归一 + 校验标签（作品表单填完那一下走这里）。
///
/// 空串是合法的：**一个标签都不填**是常态，不是错。
pub fn normalize_tags(raw: &str) -> Result<Vec<String>> {
    let tags = parse_tags(raw);
    if tags.len() > MAX_TAGS {
        return Err(Error::invalid_with(
            codes::WORK_TOO_MANY_TAGS,
            [("max", MAX_TAGS.to_string()), ("got", tags.len().to_string())],
        ));
    }
    if let Some(long) = tags.iter().find(|tag| tag.chars().count() > MAX_TAG_CHARS) {
        return Err(Error::invalid_with(
            codes::WORK_TAG_TOO_LONG,
            [("max", MAX_TAG_CHARS.to_string()), ("tag", long.clone())],
        ));
    }
    Ok(tags)
}

/// 归一 + 校验体裁（**空是合法的**：没填过就是没填过）。
///
/// 只去首尾空白、只查长度——**不查"认不认识"**：体裁是开放的（见 `db::migrations_v20`）。
/// 内部的空白原样留着（`社会 科幻` 是作者写的，软件不替他合并）。
pub fn normalize_genre(raw: &str) -> Result<String> {
    let genre = raw.trim();
    if genre.chars().count() > MAX_GENRE_CHARS {
        return Err(Error::invalid_with(
            codes::WORK_GENRE_TOO_LONG,
            [("max", MAX_GENRE_CHARS.to_string()), ("got", genre.chars().count().to_string())],
        ));
    }
    Ok(genre.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keeps_the_authors_order() {
        assert_eq!(parse_tags("悬疑\n都市\n"), vec!["悬疑", "都市"]);
    }

    #[test]
    fn parse_takes_all_three_line_endings() {
        assert_eq!(parse_tags("甲\r\n乙\r丙\n丁"), vec!["甲", "乙", "丙", "丁"]);
    }

    #[test]
    fn parse_drops_blanks_and_duplicates() {
        assert_eq!(parse_tags("  科幻 \n\n科幻\n  \n硬科幻"), vec!["科幻", "硬科幻"]);
    }

    #[test]
    fn commas_and_spaces_inside_a_tag_are_kept() {
        // 逗号、空格都不是分隔符——一行才是一个标签
        assert_eq!(parse_tags("科幻, 太空歌剧"), vec!["科幻, 太空歌剧"]);
        assert_eq!(parse_tags("硬 科幻"), vec!["硬 科幻"]);
    }

    #[test]
    fn join_and_parse_roundtrip() {
        let tags = normalize_tags("甲\n乙\n甲").unwrap();
        assert_eq!(join_tags(&tags), "甲\n乙");
        assert_eq!(parse_tags(&join_tags(&tags)), tags);
    }

    #[test]
    fn too_many_tags_is_refused_not_truncated() {
        let raw: Vec<String> = (1..=MAX_TAGS + 1).map(|i| format!("标签{i}")).collect();
        let err = normalize_tags(&raw.join("\n")).unwrap_err();
        assert!(err.to_string().contains("标签"), "报错要说清是标签那一栏：{err}");
    }

    #[test]
    fn exactly_the_limit_is_fine() {
        let raw: Vec<String> = (1..=MAX_TAGS).map(|i| format!("标签{i}")).collect();
        assert_eq!(normalize_tags(&raw.join("\n")).unwrap().len(), MAX_TAGS);
    }

    #[test]
    fn an_overlong_tag_is_refused_not_cut() {
        let long = "长".repeat(MAX_TAG_CHARS + 1);
        assert!(normalize_tags(&long).is_err(), "超长的标签要拒，不许悄悄截断");
        assert!(normalize_tags(&"长".repeat(MAX_TAG_CHARS)).is_ok(), "正好到限应当收下");
    }

    #[test]
    fn empty_is_legal_for_both() {
        assert!(normalize_tags("").unwrap().is_empty());
        assert_eq!(normalize_genre("   ").unwrap(), "");
    }

    #[test]
    fn genre_is_trimmed_but_not_rewritten_inside() {
        assert_eq!(normalize_genre("  社会 科幻 ").unwrap(), "社会 科幻");
    }

    #[test]
    fn an_overlong_genre_is_refused() {
        assert!(normalize_genre(&"体".repeat(MAX_GENRE_CHARS + 1)).is_err());
        assert!(normalize_genre(&"体".repeat(MAX_GENRE_CHARS)).is_ok());
    }
}
