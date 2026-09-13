//! 标题里的**自动编号宏**：`第{$N}章 灯` 在显示/导出时渲染成 `第3章 灯`。
//!
//! # 为什么"号"不写死在标题里
//!
//! 号一旦写死成文本，它同时成了**数据**与**位置依据**：于是每加一个场景（中间插一章、
//! 删一章、从回收站捞回、作者改用中文数字或繁体），都要再加一条"把号读回来再决定放哪儿"
//! 的规则——规则数随场景数增长，两边一旦不同步就出乱子（真机踩过：同层 18/19/20/21 时
//! 在第20章上点「+」得到 `20 / 22 / 23 / 21`）。
//!
//! 宏把号交还给**位置**：标题只存模板，渲染时按同层位置算。于是插入、删除、拖动、捞回
//! **都不需要任何"归位"逻辑**——号永远等于位置。
//!
//! # 语法（与 WonderPen 一致，另加一个补零扩展）
//!
//! | 写法 | 含义 |
//! |---|---|
//! | `{$N}` | 同层序号，从 1 开始 |
//! | `{$N0}` | 从 0 开始 |
//! | `{$N+49}` / `{$N-1}` | 偏移（第一卷写了 100 章，第二卷想从 101 起 → `{$N+100}` 或 `{$N_RESET:101}`） |
//! | `{$N_ZH}` / `{$N0_ZH}` / `{$N_ZH+49}` | 中文数字（`第一章`） |
//! | `{$N:3}` | 补零到 3 位（`001`）——我们自己的扩展 |
//! | `{$N_RESET:101}` | 从**这一章起（含它）**从 101 开始编号；不写数字就是 `{$N_RESET}`（从 1 重来） |
//!
//! # 谁占号：一条能同时管住"卷"和"序章"的规矩
//!
//! - **容器（卷 / 部）按位置排**：它们天生就是"第1卷 / 第2卷"，作者给它起了名字（`夜行`）
//!   也照样占一个号——所以第二卷仍然是第 2 卷，不会退成第 1 卷。
//! - **叶子章只有带计数宏才占号**：`序章` / `楔子` / `番外` / 散文集里自起的名字**不占号、不推号**，
//!   所以「序章 → 第{$N}章」渲染出来是「序章 → 第1章」（而不是第2章）。
//! - 不认识的 `{...}` 原样留着，**不动作者的字**。
//!
//! 纯函数、零依赖：可以脱离数据库单测。

use crate::model::NamingStyle;

/// 一个可识别的宏。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Macro {
    /// 计数：`base0` 从 0 起、`offset` 偏移、`pad` 补零位数、`zh` 中文数字
    Counter { base0: bool, offset: i64, pad: usize, zh: bool },
    /// `{$N_RESET:n}`：这一章起从 n 重新编号
    Reset(i64),
}

/// 找出标题里的宏（含它的字节区间）。不认识的 `{$...}` 不返回。
fn macros(title: &str) -> Vec<(usize, usize, Macro)> {
    let mut out = Vec::new();
    let bytes = title.as_bytes();
    let mut at = 0usize;
    while let Some(start) = title[at..].find("{$") {
        let start = at + start;
        let Some(close) = title[start..].find('}') else { break };
        let end = start + close + 1; // 含 '}'
        if let Some(found) = parse_macro(&title[start + 2..end - 1]) {
            out.push((start, end, found));
        }
        at = end;
        if at >= bytes.len() {
            break;
        }
    }
    out
}

/// 解析 `{$…}` 里面的内容；认不出来就是 `None`（原样留在标题里）。
fn parse_macro(body: &str) -> Option<Macro> {
    let (base0, rest) = if let Some(rest) = body.strip_prefix("N0") {
        (true, rest)
    } else if let Some(rest) = body.strip_prefix('N') {
        (false, rest)
    } else {
        return None;
    };
    if let Some(rest) = rest.strip_prefix("_RESET") {
        // `{$N_RESET}` = 从 1 重来；`{$N_RESET:101}` = 从 101 开始
        let start = match rest.strip_prefix(':') {
            Some(number) => number.parse().ok()?,
            None if rest.is_empty() => 1,
            None => return None,
        };
        return Some(Macro::Reset(start));
    }
    let (zh, rest) = match rest.strip_prefix("_ZH") {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let (pad, rest) = match rest.strip_prefix(':') {
        Some(number) => (number.parse().ok()?, ""),
        None => (0usize, rest),
    };
    let offset = if rest.is_empty() {
        0
    } else if let Some(number) = rest.strip_prefix('+') {
        number.parse().ok()?
    } else if let Some(number) = rest.strip_prefix('-') {
        -number.parse::<i64>().ok()?
    } else {
        return None;
    };
    Some(Macro::Counter { base0, offset, pad, zh })
}

/// 这个标题里有没有计数宏——**有才算"这一章要编号"**（没宏的不占号、不推号）。
pub fn has_counter(title: &str) -> bool {
    macros(title)
        .iter()
        .any(|(_, _, found)| matches!(found, Macro::Counter { .. }))
}

/// 这个标题要从几开始重新编号（`{$N_RESET:n}`）；没有就是 `None`。
pub fn reset_at(title: &str) -> Option<i64> {
    macros(title).iter().find_map(|(_, _, found)| match found {
        Macro::Reset(start) => Some(*start),
        _ => None,
    })
}

/// 把标题里的宏换成具体数字；`{$N_RESET:…}` 本身渲染成空（它是给计数器的指令，不是文字）。
///
/// 认不出来的宏（作者自己写的 `{...}`）原样留着——**不动作者的字**。
pub fn render(title: &str, number: i64) -> String {
    let found = macros(title);
    let mut out = String::with_capacity(title.len());
    let mut at = 0usize;
    for (start, end, found) in found {
        out.push_str(&title[at..start]);
        match found {
            Macro::Reset(_) => {}
            Macro::Counter { base0, offset, pad, zh } => {
                let mut value = number + offset - i64::from(base0);
                if value < 0 {
                    value = 0;
                }
                if zh {
                    // 中文数字不补零（`{$N_ZH:3}` 这种写法按中文来：`一`）
                    out.push_str(&cn(value));
                } else if pad > 0 {
                    out.push_str(&format!("{value:0width$}", width = pad));
                } else {
                    out.push_str(&value.to_string());
                }
            }
        }
        at = end;
    }
    out.push_str(&title[at..]);
    out
}

/// 层里的一项。
pub struct LayerItem<'a> {
    /// 标题原文（可能是模板）
    pub title: &'a str,
    /// **不带计数宏也占号吗**——卷 / 部这类容器传 `true`（它们按位置排），叶子章传 `false`
    pub positional: bool,
}

/// 把**一层**渲染出来。层内的顺序就是目录顺序（调用方保证）。
///
/// 占号的两类：带计数宏的、以及 `positional` 的（容器）。`{$N_RESET:n}` 从它自己开始生效。
pub fn render_layer(items: &[LayerItem<'_>]) -> Vec<String> {
    let mut counter = 1i64;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Some(start) = reset_at(item.title) {
            counter = start;
        }
        // 一律过一遍 render：带 RESET 但不带计数宏的节点，也要把那条**指令**抹掉
        // （它不是作者想显示的字）；没有任何宏时 render 原样返回。
        out.push(render(item.title, counter));
        if item.positional || has_counter(item.title) {
            counter += 1;
        }
    }
    out
}

// i18n-allow-begin: 这是**中文数字的字表**——渲染 `{$N_ZH}` 用的数据（写进标题的字），
// 不是界面文案：换界面语言也不该换它（章号的中文写法属于作者的数据）。
const DIGITS: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const UNITS: [&str; 4] = ["", "十", "百", "千"];
// i18n-allow-end

/// 中文数字：`1` → `一`、`12` → `十二`、`20` → `二十`、`101` → `一百零一`、`1234` → `一千二百三十四`。
///
/// 只做到四位（一万章的书不存在）；超出范围原样返回阿拉伯数字——**宁可露出数字，也不猜**。
pub fn cn(number: i64) -> String {
    if number == 0 {
        return DIGITS[0].to_string();
    }
    if !(1..=9999).contains(&number) {
        return number.to_string();
    }
    let digits: Vec<usize> = number
        .to_string()
        .chars()
        .map(|c| c.to_digit(10).unwrap_or(0) as usize)
        .collect();
    let length = digits.len();
    let mut out = String::new();
    let mut zero_pending = false;
    for (at, digit) in digits.iter().enumerate() {
        let unit = length - 1 - at;
        if *digit == 0 {
            if !out.is_empty() {
                zero_pending = true;
            }
            continue;
        }
        if zero_pending {
            out.push_str(DIGITS[0]);
            zero_pending = false;
        }
        // 「十二」而不是「一十二」：只有"十位且是 1、前面还没写过东西"才省掉那个一
        if !(unit == 1 && *digit == 1 && out.is_empty()) {
            out.push_str(DIGITS[*digit]);
        }
        out.push_str(UNITS[unit]);
    }
    out
}

/// 把标题里**已有的计数宏**换成 `style` 那一档，其余文字一个字不动。
///
/// 偏移与"从 0 起"是**作者对某章的特意安排**，换档时保留（`{$N+49}` → `{$N_ZH+49}`）；
/// 补零是"哪一档写法"的一部分，所以跟着目标走（换成阿拉伯就 `{$N}`、换成中文就 `{$N_ZH}`）。
///
/// 返回 `None` = 不用改（没有宏，或者本来就是这一档）。
pub fn rewrite_counter(title: &str, style: NamingStyle) -> Option<String> {
    if style == NamingStyle::NoNumber {
        return None; // 见 rewrite_literal 的说明
    }
    let found = macros(title);
    if !found.iter().any(|(_, _, found)| matches!(found, Macro::Counter { .. })) {
        return None;
    }
    let mut out = String::with_capacity(title.len());
    let mut at = 0usize;
    for (start, end, found) in found {
        out.push_str(&title[at..start]);
        match found {
            Macro::Reset(_) => out.push_str(&title[start..end]), // 重置指令原样留着
            Macro::Counter { base0, offset, pad, .. } => {
                let body = match style {
                    NamingStyle::Arabic => format!("N{}", sign(offset)),
                    NamingStyle::Chinese => format!("N_ZH{}", sign(offset)),
                    NamingStyle::Padded => format!("N:3{}", sign(offset)),
                    NamingStyle::NoNumber => String::new(),
                };
                let body = if base0 { body.replacen('N', "N0", 1) } else { body };
                let _ = pad;
                out.push_str(&format!("{{${body}}}"));
            }
        }
        at = end;
    }
    out.push_str(&title[at..]);
    (out != title).then_some(out)
}

/// `+49` / `-1` / 空。
fn sign(offset: i64) -> String {
    match offset {
        0 => String::new(),
        n if n > 0 => format!("+{n}"),
        n => n.to_string(),
    }
}

/// 把**手写的编号**（`第12章 灯`）换成宏（`第{$N}章 灯`），其余文字一个字不动。
///
/// 只认**阿拉伯数字**：中文数字那种（`第十三章`）本来就是"中文档"的样子，不用换；
/// 认不出的一概返回 `None`（序章 / 楔子 / 番外 / 自定义标题都在这条路上，一个字都不动）。
///
/// ⚠️ `style = NoNumber`（不编号）时返回 `None`：那是"以后新建的不编号"，
/// **把已有章的名字抹掉不是这个动作该干的事**。
pub fn rewrite_literal(title: &str, prefix: &str, suffix: &str, style: NamingStyle) -> Option<String> {
    if style == NamingStyle::NoNumber || prefix.is_empty() {
        return None;
    }
    let rest = title.strip_prefix(prefix)?;
    let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    // 「第 3 节」中间可以有空格：数字与后缀各自 trim 一下再对
    let after_number = rest.trim_start()[digits.len()..].trim_start();
    let name = after_number.strip_prefix(suffix.trim())?;
    let counter = style.counter();
    let out = format!("{prefix}{counter}{suffix}{name}");
    (out != title).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 叶子层（章 / 篇）：不带宏的不占号
    fn layer(titles: &[&str]) -> Vec<String> {
        let items: Vec<LayerItem<'_>> = titles
            .iter()
            .map(|title| LayerItem { title, positional: false })
            .collect();
        render_layer(&items)
    }

    /// 容器层（卷 / 部）：按位置排，起了名字也占号
    fn volumes(titles: &[&str]) -> Vec<String> {
        let items: Vec<LayerItem<'_>> =
            titles.iter().map(|title| LayerItem { title, positional: true }).collect();
        render_layer(&items)
    }

    #[test]
    fn arabic_counter_follows_position() {
        assert_eq!(layer(&["第{$N}章", "第{$N}章", "第{$N}章"]), ["第1章", "第2章", "第3章"]);
        // 中间插一章、删一章：只要位置在，号就跟着走——**不需要任何"归位"**
        assert_eq!(
            layer(&["第{$N}章", "第{$N}章 新插的", "第{$N}章"]),
            ["第1章", "第2章 新插的", "第3章"]
        );
    }

    #[test]
    fn containers_count_by_position_even_when_they_have_a_name() {
        // 作者把卷起成「夜行」「归途」，第三卷仍旧是 第3卷（不会退成 第1卷）
        assert_eq!(volumes(&["夜行", "归途", "第{$N}卷"]), ["夜行", "归途", "第3卷"]);
        assert_eq!(volumes(&["第{$N}卷", "第{$N}卷"]), ["第1卷", "第2卷"]);
    }

    #[test]
    fn counter_only_counts_the_ones_that_carry_the_macro() {
        // 序章 / 番外 / 散文里的自起名**不占号、也不推号**
        assert_eq!(
            layer(&["序章", "第{$N}章", "第{$N}章", "番外"]),
            ["序章", "第1章", "第2章", "番外"]
        );
        assert_eq!(layer(&["故乡的野菜", "背影"]), ["故乡的野菜", "背影"]);
    }

    #[test]
    fn offsets_zero_based_padding_and_chinese_numbers() {
        assert_eq!(layer(&["第{$N0}章"]), ["第0章"]);
        assert_eq!(layer(&["第{$N+49}章"]), ["第50章"], "第一卷写了 100 章时可以这样接上");
        assert_eq!(layer(&["第{$N:3}章", "第{$N:3}章"]), ["第001章", "第002章"]);
        assert_eq!(
            layer(&["第{$N_ZH}章", "第{$N_ZH}章", "第{$N_ZH}章"]),
            ["第一章", "第二章", "第三章"]
        );
        assert_eq!(layer(&["第{$N0_ZH}章"]), ["第零章"]);
        assert_eq!(layer(&["第{$N_ZH+49}章"]), ["第五十章"]);
        assert_eq!(layer(&["第{$N_ZH:3}章"]), ["第一章"], "中文数字不补零（补零是阿拉伯数字的事）");
    }

    #[test]
    fn reset_restarts_the_counter_from_that_node_on() {
        // 现实里写成"`{$N}` 与重置指令同一行"（与 WonderPen 文档里的例子一致）
        assert_eq!(
            layer(&["第{$N}章", "第{$N}章", "第{$N}章{$N_RESET:101}", "第{$N}章"]),
            ["第1章", "第2章", "第101章", "第102章"],
            "第二卷从 101 起号：重置从它自己这一章开始生效"
        );
        assert_eq!(
            layer(&["第{$N}章", "第{$N}章{$N_RESET}", "第{$N}章"]),
            ["第1章", "第1章", "第2章"],
            "`{{$N_RESET}}`（不带数字）= 从 1 重来"
        );
        // 只写重置、不写 `{$N}`：那是指令，不是文字——它不显示、也不占号，
        // 但从它开始后面接着它给的数字（作者要显示号就得同时写 `{$N}`）
        assert_eq!(layer(&["第{$N}章", "第{$N_RESET:101}章", "第{$N}章"]), ["第1章", "第章", "第101章"]);
    }

    #[test]
    fn switching_the_style_rewrites_the_macro_and_keeps_the_name() {
        // 作者写了 30 章阿拉伯数字，改主意想要中文数字：换档只动编号，章名一个字不动
        assert_eq!(
            rewrite_counter("第{$N}章 灯", NamingStyle::Chinese).as_deref(),
            Some("第{$N_ZH}章 灯")
        );
        assert_eq!(
            rewrite_counter("第{$N_ZH}章", NamingStyle::Padded).as_deref(),
            Some("第{$N:3}章")
        );
        assert_eq!(
            rewrite_counter("第{$N:3}章 门", NamingStyle::Arabic).as_deref(),
            Some("第{$N}章 门")
        );
        // 偏移与"从 0 起"是作者对某章的特意安排：换档保留
        assert_eq!(
            rewrite_counter("第{$N+49}章", NamingStyle::Chinese).as_deref(),
            Some("第{$N_ZH+49}章")
        );
        assert_eq!(
            rewrite_counter("第{$N0}章", NamingStyle::Chinese).as_deref(),
            Some("第{$N0_ZH}章")
        );
        // 已经是这一档 / 压根没有宏：不用改
        assert_eq!(rewrite_counter("第{$N}章", NamingStyle::Arabic), None);
        assert_eq!(rewrite_counter("序章", NamingStyle::Chinese), None);
        // "不编号"不动已有章的名字
        assert_eq!(rewrite_counter("第{$N}章", NamingStyle::NoNumber), None);
    }

    #[test]
    fn hand_written_numbers_become_macros() {
        assert_eq!(
            rewrite_literal("第12章 灯", "第", "章", NamingStyle::Arabic).as_deref(),
            Some("第{$N}章 灯")
        );
        assert_eq!(
            rewrite_literal("第 3 节", "第", "节", NamingStyle::Chinese).as_deref(),
            Some("第{$N_ZH}节")
        );
        // 认不出来的一概不动
        assert_eq!(rewrite_literal("序章", "第", "章", NamingStyle::Arabic), None);
        assert_eq!(rewrite_literal("第十三章", "第", "章", NamingStyle::Arabic), None);
        assert_eq!(rewrite_literal("第12章", "第", "章", NamingStyle::NoNumber), None);
    }

    #[test]
    fn unknown_braces_are_left_alone() {
        // 作者自己写的 {...} 不许被我们瞎替换；认不出的宏也不参与计数
        assert_eq!(layer(&["第{$N}章 {自定义}", "第{$N}章"]), ["第1章 {自定义}", "第2章"]);
        assert_eq!(layer(&["{N}"]), ["{N}"], "没有 $ 的不算宏");
    }

    #[test]
    fn chinese_numbers_read_like_people_say_them() {
        for (number, want) in [
            (1, "一"),
            (9, "九"),
            (10, "十"),
            (11, "十一"),
            (20, "二十"),
            (21, "二十一"),
            (100, "一百"),
            (101, "一百零一"),
            (110, "一百一十"),
            (111, "一百一十一"),
            (1000, "一千"),
            (1005, "一千零五"),
            (1050, "一千零五十"),
            (1234, "一千二百三十四"),
            (9999, "九千九百九十九"),
        ] {
            assert_eq!(cn(number), want, "{number} 的中文写法");
        }
        assert_eq!(cn(0), "零");
        assert_eq!(cn(10000), "10000", "超出四位就露数字，不猜");
    }
}
