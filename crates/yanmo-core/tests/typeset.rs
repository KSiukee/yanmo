//! 中文排版规范化的验收：**只报告、可逐条确认、且改完是干净的**。
//!
//! 四件事必须成立：
//! 1. 每条规则只在自己那一亩三分地里动手——英文缩写、小数、版本号、代码参数一律不碰；
//! 2. 作者没勾的条目**一个字都不许改**（勾选是逐条的）；
//! 3. 应用过一遍之后再扫必须**一条不剩**（幂等，同一处不会被反复改来改去）；
//! 4. **没有确定改法的一律只提醒不改稿**（引号括号缺一半：补哪边都是猜）。

use yanmo_core::typeset::{self, Change, Notice, Options, Report, RuleInfo};

fn opts(style: Option<&str>) -> Options {
    Options { quote_style: style.map(str::to_string) }
}

fn report(text: &str, options: &Options) -> Report {
    typeset::scan(text, options).unwrap()
}

fn changes(text: &str, options: &Options) -> Vec<Change> {
    report(text, options).changes
}

fn notices(text: &str, options: &Options) -> Vec<Notice> {
    report(text, options).notices
}

fn rules_of(text: &str, options: &Options) -> Vec<&'static str> {
    changes(text, options).iter().map(|change| change.rule).collect()
}

/// 扫一遍、把能改的全勾上应用（模拟"全选后点应用"）。
fn fix_all(text: &str, options: &Options) -> String {
    let all: Vec<usize> = (0..changes(text, options).len()).collect();
    typeset::apply(text, options, &all).unwrap()
}

/// 只把某一条规则的改动应用掉。
fn fix_rule(text: &str, options: &Options, rule: &str) -> String {
    let picked: Vec<usize> = changes(text, options)
        .iter()
        .enumerate()
        .filter(|(_, change)| change.rule == rule)
        .map(|(index, _)| index)
        .collect();
    typeset::apply(text, options, &picked).unwrap()
}

/// 某条规则的风险档（界面据此决定默认勾不勾）。
fn tier_of(code: &str) -> &'static str {
    typeset::RULES
        .iter()
        .find(|rule: &&RuleInfo| rule.code == code)
        .map(|rule| rule.tier)
        .unwrap_or("")
}

#[test]
fn ellipsis_becomes_two_groups_of_dots() {
    let o = opts(None);
    assert_eq!(fix_all("他走了...", &o), "他走了……");
    assert_eq!(fix_all("他走了。。。", &o), "他走了……");
    assert_eq!(fix_all("他走了…", &o), "他走了……");
    // 已经规范的、以及单个句号，都不动
    assert_eq!(fix_all("他走了……", &o), "他走了……");
    assert_eq!(fix_all("他走了。", &o), "他走了。");
    // 英文缩写里的点只有两个性质，不算省略号
    assert!(rules_of("e.g. 这样", &o).is_empty(), "{:?}", rules_of("e.g. 这样", &o));
}

#[test]
fn dash_becomes_two_em_dashes() {
    let o = opts(None);
    assert_eq!(fix_all("他来了--又走了", &o), "他来了——又走了");
    assert_eq!(fix_all("他来了—又走了", &o), "他来了——又走了");
    assert_eq!(fix_all("他来了——又走了", &o), "他来了——又走了");
    // 命令行参数里的双连字符不动
    assert_eq!(fix_all("跑一遍 --self-test", &o), "跑一遍 --self-test");
}

#[test]
fn leading_blanks_go_away_but_blank_lines_stay() {
    let o = opts(None);
    assert_eq!(fix_all("　　第一段\n\n 第二段", &o), "第一段\n\n第二段");
    // 空行是段落分隔，不能被吃掉
    assert_eq!(fix_all("第一段\n\n\n第二段", &o), "第一段\n\n\n第二段");
    // 段中的空格不动
    assert_eq!(fix_all("他说 你好", &o), "他说 你好");
}

#[test]
fn halfwidth_punctuation_after_chinese_goes_fullwidth() {
    let o = opts(None);
    // 只跑这一条规则：中英之间的空格是另一条（风格档），不该混进来
    let fix = |text: &str| fix_rule(text, &o, "halfwidth_punct");
    assert_eq!(fix("他说,好的.然后?"), "他说，好的。然后？");
    assert_eq!(fix("这是(中文)"), "这是（中文）");
    // 数字、版本号、网址、括号包英文：都保持原样
    assert_eq!(fix("圆周率是3.14"), "圆周率是3.14");
    assert_eq!(fix("版本v0.16.2"), "版本v0.16.2");
    assert_eq!(fix("网址是http://a.b"), "网址是http://a.b");
    assert_eq!(fix("这是(English)"), "这是(English)");
}

#[test]
fn repeated_punctuation_collapses_to_one() {
    let o = opts(None);
    assert_eq!(fix_all("真的吗？？然后！！", &o), "真的吗？然后！");
    // 中文语境里连着打的半角标点，收成一个全角
    assert_eq!(fix_all("好??", &o), "好？");
    // 省略号与破折号不是"重复标点"
    assert_eq!(fix_all("他走了……——", &o), "他走了……——");
}

#[test]
fn quotes_are_unified_to_the_chosen_style() {
    let curly = opts(None);
    assert_eq!(fix_all("他说:\"你好\"", &curly), "他说：“你好”");
    // 落单的一只留着不动：宁可不改，也不制造没配对的引号（半角冒号照样收成全角）
    assert_eq!(fix_all("他说:\"你好", &curly), "他说：\"你好");
    // 英文撇号不是引号
    assert_eq!(fix_all("don't stop", &curly), "don't stop");

    let corner = opts(Some("corner"));
    assert_eq!(fix_all("他说：“你好”", &corner), "他说：「你好」");
    assert_eq!(fix_all("他说:\"你好\"", &corner), "他说：「你好」");
}

#[test]
fn doubled_function_chars_and_words_collapse() {
    let o = opts(None);
    assert_eq!(fix_all("他说的的确是这样", &o), "他说的确是这样");
    assert_eq!(fix_all("已经已经来不及了", &o), "已经来不及了");
    assert_eq!(fix_all("因为因为下雨", &o), "因为下雨");
    // 合法的叠字、叠词一个都不许碰
    assert!(rules_of("他看了看，慢慢走过来", &o).is_empty(), "{:?}", rules_of("他看了看", &o));
    assert!(rules_of("大家商量商量再说", &o).is_empty(), "{:?}", rules_of("商量商量", &o));
}

#[test]
fn a_stutter_is_offered_but_never_checked_by_default() {
    let o = opts(None);
    // 对话里的结巴（"你你你说什么"）是修辞：照样列出来给作者看，但它属于"自己勾"那一档
    let hit = changes("你你你说什么", &o);
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].rule, "repeat_char");
    assert_eq!(tier_of("repeat_char"), "careful");
    // 而"建议改"那一档里没有它
    assert!(tier_of("ellipsis") == "safe" && tier_of("repeat_word") == "careful");
}

#[test]
fn a_missing_quote_is_reported_but_never_fixed() {
    let o = opts(None);
    let curly = report("他说：“你好", &o);
    assert!(curly.changes.is_empty(), "缺一半的引号没有确定改法：{:?}", curly.changes);
    assert_eq!(curly.notices.len(), 1);
    assert_eq!(curly.notices[0].rule, "pair_missing");
    assert_eq!(curly.notices[0].mark, "quote_double");
    assert_eq!(curly.notices[0].side, "unclosed");

    // 反过来：多了一个右引号
    let extra = notices("他说：你好”", &o);
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0].side, "unopened");

    // 提醒归提醒，正文一个字都不许动
    assert_eq!(fix_all("他说：“你好", &o), "他说：“你好");
}

#[test]
fn pairing_is_counted_per_paragraph_and_quiet_when_balanced() {
    let o = opts(None);
    let found = notices("他说：“你好\n\n她又说：「走吧", &o);
    assert_eq!(found.len(), 2, "两段各缺一处：{found:?}");
    assert_eq!(found[0].paragraph, 1);
    assert_eq!(found[1].paragraph, 2);
    assert_eq!(found[1].mark, "corner");

    // 配对齐全（含书名号、括号、角引号）就不该有提醒
    assert!(notices("《长夜》里写：「他说（好）。」", &o).is_empty());
}

#[test]
fn cjk_latin_spacing_is_one_rule_of_its_own() {
    let o = opts(None);
    assert_eq!(fix_rule("中文English混排", &o, "cjk_latin_space"), "中文 English 混排");
    // 没勾这一条时，它一个字都不许动
    assert_eq!(fix_rule("中文English混排", &o, "ellipsis"), "中文English混排");
}

#[test]
fn one_spot_is_claimed_by_one_rule_only() {
    let o = opts(None);
    let hit = changes("他走了。。。", &o);
    assert_eq!(hit.len(), 1, "同一处只该出一条：{hit:?}");
    assert_eq!(hit[0].rule, "ellipsis");
    assert_eq!(hit[0].before, "。。。");
    assert_eq!(hit[0].after, "……");
}

#[test]
fn a_change_knows_which_paragraph_it_is_in() {
    let o = opts(None);
    let hit = changes("第一段\n\n第二段...", &o);
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].paragraph, 2);
    assert_eq!(hit[0].context_before, "第二段");
    assert_eq!(hit[0].context_after, "");
}

#[test]
fn applying_everything_then_scanning_again_finds_nothing() {
    let o = opts(None);
    let text = "　　他说,\"你好...\"--然后又喊\"走??\" 中文English。。";
    let once = fix_all(text, &o);
    let left = report(&once, &o);
    assert!(
        left.changes.is_empty() && left.notices.is_empty(),
        "应用过一遍还剩 {left:?}（原稿 {text} → {once}）"
    );
}

#[test]
fn only_the_checked_changes_are_applied() {
    let o = opts(None);
    let text = "他走了...然后 中文English";
    let hit = changes(text, &o);
    assert!(hit.len() > 1, "这份文本本该有多处命中");
    // 只勾第一条（文档顺序里的第一处：省略号）
    assert_eq!(typeset::apply(text, &o, &[0]).unwrap(), "他走了……然后 中文English");
    // 一条都不勾＝原样奉还
    assert_eq!(typeset::apply(text, &o, &[]).unwrap(), text);
}

#[test]
fn a_stale_preview_is_refused_whole() {
    let o = opts(None);
    // 稿子改过之后，旧的勾选序号就对不上了：整批拒绝，由界面重新预览
    let error = typeset::apply("这一段没有任何毛病", &o, &[0]).unwrap_err();
    assert_eq!(error.code(), "typeset.change_unknown");
}

#[test]
fn an_unknown_quote_style_is_refused() {
    let bad = opts(Some("angled"));
    let error = typeset::scan("他说:\"你好\"", &bad).unwrap_err();
    assert_eq!(error.code(), "value.unknown_quote_style");
}

#[test]
fn a_clean_manuscript_yields_no_changes() {
    let o = opts(None);
    let text = "他推开门，屋里没有人。\n\n桌上的茶还温着——刚走不久。";
    assert!(typeset::scan(text, &o).unwrap().changes.is_empty());
    assert_eq!(fix_all(text, &o), text);
}
