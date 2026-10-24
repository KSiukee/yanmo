//! 外观偏好验收：**全局打底 + 每书可选覆盖**，只存改过的项，坏记录当没设过。
//!
//! 四条判据：
//! 1. 没设过 → 默认值（默认只有一处，读的时候才落）；
//! 2. 写进去的只留改过的项（稀疏），全改回"没设过"就把键清掉；
//! 3. 书的覆盖 **只覆盖它真设过的项**，其余继承全局；
//! 4. 坏 JSON 不报错、当没设过——界面不该被一条坏记录卡住。

use yanmo_core::model::{NodeKind, RuleStyle, SideTab, WorkKind};
use yanmo_core::store::{Appearance, Store};
use yanmo_core::text::WordCaliber;
use yanmo_core::typeset::QuoteStyle;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 库里某个键的原文（没这个键就是 None）。
fn raw(store: &Store, key: &str) -> Option<String> {
    store
        .conn()
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

#[test]
fn defaults_apply_when_nothing_was_ever_set() {
    let (_dir, store) = fresh();
    let prefs = store.appearance(None).unwrap();
    assert!(prefs.jump_to_end_on_latest, "默认开");
    assert!(raw(&store, "appearance").is_none(), "读默认不该顺手写库");
}

#[test]
fn writes_stay_sparse_and_clear_when_back_to_default() {
    let (_dir, mut store) = fresh();
    let off = Appearance { jump_to_end_on_latest: Some(false), ..Default::default() };
    store.set_appearance(None, &off).unwrap();

    assert!(!store.appearance(None).unwrap().jump_to_end_on_latest);
    let stored = raw(&store, "appearance").expect("改过就该有记录");
    assert!(stored.contains("jump_to_end_on_latest"));
    assert!(!stored.contains("null"), "只存改过的项，不写空档：{stored}");

    // 空 patch 是"这项不改"（稀疏语义）：键还在
    store.set_appearance(None, &Appearance::default()).unwrap();
    assert!(raw(&store, "appearance").is_some(), "空 patch 不该动已有记录");

    // 真的要"回到默认"要用 reset：键被清掉，读出来又是默认值
    store.reset_appearance(None).unwrap();
    assert!(raw(&store, "appearance").is_none(), "回到默认就该把键删掉");
    assert!(store.appearance(None).unwrap().jump_to_end_on_latest, "回到默认");
}

#[test]
fn a_books_override_only_covers_what_it_really_set() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    // 全局关掉；这本书单独打开
    store.set_appearance(None, &Appearance { jump_to_end_on_latest: Some(false), ..Default::default() }).unwrap();
    store
        .set_appearance(Some(work.id), &Appearance { jump_to_end_on_latest: Some(true), ..Default::default() })
        .unwrap();

    assert!(store.appearance(Some(work.id)).unwrap().jump_to_end_on_latest, "这本书的覆盖盖住全局");
    assert!(!store.appearance(None).unwrap().jump_to_end_on_latest, "全局没被动过");
    assert!(raw(&store, &format!("work.{}.appearance", work.id)).is_some(), "覆盖按书写键");

    // 另一本书没单独设过：继承全局（关）
    let other = store.create_work(WorkKind::Novel, "别的书").unwrap();
    assert!(!store.appearance(Some(other.id)).unwrap().jump_to_end_on_latest, "没设过就继承全局");
}

#[test]
fn a_broken_record_falls_back_to_defaults_instead_of_failing() {
    let (_dir, mut store) = fresh();
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('appearance', '这不是 JSON', 0)",
            [],
        )
        .unwrap();

    let prefs = store.appearance(None).unwrap();
    assert!(prefs.jump_to_end_on_latest, "读不出来当没设过：宁可回默认也不卡住界面");

    // 坏记录还能被正常覆盖写掉
    store
        .set_appearance(None, &Appearance { jump_to_end_on_latest: Some(false), ..Default::default() })
        .unwrap();
    assert!(!store.appearance(None).unwrap().jump_to_end_on_latest);
}

#[test]
fn word_count_caliber_is_stored_per_book_and_defaults_to_none() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    // 没设过 → None（界面拿作品语言的默认口径顶上，核心不替它猜）
    assert_eq!(store.appearance(None).unwrap().word_count_caliber, None);

    // 全局设「不含标点」，这本书单独设「按词」：覆盖照常生效
    store
        .set_appearance(
            None,
            &Appearance { word_count_caliber: Some("chars_no_punct".into()), ..Default::default() },
        )
        .unwrap();
    store
        .set_appearance(
            Some(work.id),
            &Appearance { word_count_caliber: Some("words".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(
        store.appearance(Some(work.id)).unwrap().word_count_caliber,
        Some(WordCaliber::Words)
    );
    assert_eq!(
        store.appearance(None).unwrap().word_count_caliber,
        Some(WordCaliber::CharsNoPunct),
        "全局没被动过"
    );

    // 另一本书没单独设过：继承全局
    let other = store.create_work(WorkKind::Novel, "别的书").unwrap();
    assert_eq!(
        store.appearance(Some(other.id)).unwrap().word_count_caliber,
        Some(WordCaliber::CharsNoPunct)
    );
}

#[test]
fn the_quote_style_is_remembered_and_a_bad_code_is_refused() {
    let (_dir, mut store) = fresh();
    // 没设过 → 默认弯引号
    assert_eq!(store.appearance(None).unwrap().quote_style, QuoteStyle::Curly);

    // 作者选了角引号：记住，下次排版清理直接用它
    store
        .set_appearance(None, &Appearance { quote_style: Some("corner".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().quote_style, QuoteStyle::Corner);

    // 不认识的要当场报错，也不许落库
    let bad = store.set_appearance(
        None,
        &Appearance { quote_style: Some("angled".into()), ..Default::default() },
    );
    assert!(bad.is_err(), "不认识的引号风格要当场报错");

    // 库里已有的坏代码当没设过（跟坏 JSON 一条规矩）
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"quote_style\":\"angled\"}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().quote_style, QuoteStyle::Curly);
}

#[test]
fn an_unusable_caliber_is_rejected_on_write_and_ignored_on_read() {
    let (_dir, mut store) = fresh();

    // 写：只认三个稳定代码——写垃圾当场报错，不往库里塞
    let bad = store.set_appearance(
        None,
        &Appearance { word_count_caliber: Some("随便写的".into()), ..Default::default() },
    );
    assert!(bad.is_err(), "不认识的口径代码要当场报错");
    assert!(raw(&store, "appearance").is_none(), "报错了就不该落库");

    // 读：库里已有的坏代码当没设过（跟坏 JSON 一条规矩）
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"word_count_caliber\":\"nonsense\"}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().word_count_caliber, None);
}

/// 命名规则：**默认按作品类型，作者选了就听作者的，每本书可以各设各的**。
///
/// 这是"新建条目叫什么"的落定规则（号本身是位置的函数，见 `numbering`）：
/// 长篇默认 `第{$N}章`；单篇与文集默认**不编号**（名字留给作者）。
#[test]
fn naming_style_follows_the_work_kind_until_the_author_chooses() {
    use yanmo_core::model::NamingStyle;

    let (_dir, mut store) = fresh();
    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let collection = store.create_work(WorkKind::Collection, "故园随笔").unwrap();

    // ① 没选过：跟作品类型
    assert_eq!(store.naming_style(novel.id).unwrap(), NamingStyle::Arabic);
    assert_eq!(store.naming_style(collection.id).unwrap(), NamingStyle::NoNumber);

    // ② 全局设成中文数字：两本都跟着变
    store
        .set_appearance(None, &Appearance { naming: Some("chinese".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.naming_style(novel.id).unwrap(), NamingStyle::Chinese);
    assert_eq!(store.naming_style(collection.id).unwrap(), NamingStyle::Chinese);

    // ③ 单本覆盖：只影响它
    store
        .set_appearance(
            Some(novel.id),
            &Appearance { naming: Some("padded".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(store.naming_style(novel.id).unwrap(), NamingStyle::Padded);
    assert_eq!(store.naming_style(collection.id).unwrap(), NamingStyle::Chinese, "另一本不受影响");

    // ④ "auto" = 清掉这一层：回到继承默认（这里继承的是全局那份 chinese）
    store
        .set_appearance(Some(novel.id), &Appearance { naming: Some("auto".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.naming_style(novel.id).unwrap(), NamingStyle::Chinese);

    // ⑤ 认不出来的代码写不进去（界面不该被埋一个"未知规则"）
    assert!(store
        .set_appearance(None, &Appearance { naming: Some("乱写的".into()), ..Default::default() })
        .is_err());
}

/// 命名规则**真的贯穿到新建的章上**：库里存的是对应模板，渲染出来才对得上。
#[test]
fn the_chosen_naming_style_shows_up_in_new_chapters() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let first = store.list_nodes(work.id).unwrap()[0].id;

    for (at, (code, raw, rendered)) in [
        ("arabic", "第{$N}章", "第1章"),
        ("chinese", "第{$N_ZH}章", "第一章"),
        ("padded", "第{$N:3}章", "第001章"),
        ("none", "", ""),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .set_appearance(
                None,
                &Appearance {
                    naming: Some(code.into()),
                    // 这一条只管**命名写法**，不掺编号方式：显式用"每卷从头数"，
                    // 四档写法才都从"第 1 章"起（跨卷延续有它自己的一组用例）
                    chapter_numbering: Some("per_volume".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        // 每种规则各放一卷：同一层里换规则会让计数接着上一档往下数（那是另一回事）
        let volume = if at == 0 {
            first
        } else {
            store.create_node(work.id, None, NodeKind::Volume, "").unwrap()
        };
        let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
        assert_eq!(store.node_title(chapter).unwrap(), raw, "{code}：库里存的模板");
        assert_eq!(store.rendered_title(chapter).unwrap(), rendered, "{code}：显示出来的样子");
    }
}

/// 正文排版（字号 / 行距 / 字距）是**纯显示层**：能记住、能每书覆盖、能夹范围，
/// **但正文与导出一个字节都不许变**（这一条是这项功能的验收底线）。
#[test]
fn typography_is_remembered_per_item_and_never_touches_the_text() {
    use yanmo_core::store::ExportFormat;
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    store.write_body(chapter, "第一段。\n第二段。").unwrap();

    let body_before = store.read_body(chapter).unwrap();
    let export_before: Vec<u8> = store
        .render_work(work.id, ExportFormat::Text)
        .unwrap()
        .iter()
        .flat_map(|file| file.content.clone())
        .collect();

    // 设三项：只写传进来的那些（没传的保持原样）
    store
        .set_appearance(
            Some(work.id),
            &Appearance {
                editor_font_size: Some(20),
                editor_line_height: Some(200),
                ..Default::default()
            },
        )
        .unwrap();
    let resolved = store.appearance(Some(work.id)).unwrap();
    assert_eq!(resolved.editor_font_size, Some(20));
    assert_eq!(resolved.editor_line_height, Some(200));
    assert_eq!(resolved.editor_letter_spacing, None, "没传的那项不该被顺手写进去");

    store
        .set_appearance(
            Some(work.id),
            &Appearance { editor_letter_spacing: Some(5), ..Default::default() },
        )
        .unwrap();
    let resolved = store.appearance(Some(work.id)).unwrap();
    assert_eq!(resolved.editor_letter_spacing, Some(5));
    assert_eq!(resolved.editor_font_size, Some(20), "上一次那两项还在");

    // ★ 正文与导出：一个字节都不许变
    assert_eq!(store.read_body(chapter).unwrap(), body_before, "排版设置不许碰正文");
    let export_after: Vec<u8> = store
        .render_work(work.id, ExportFormat::Text)
        .unwrap()
        .iter()
        .flat_map(|file| file.content.clone())
        .collect();
    assert_eq!(export_after, export_before, "排版设置不许进导出");

    // 极端值夹进可读范围（不报错、不破版）
    store
        .set_appearance(
            Some(work.id),
            &Appearance {
                editor_font_size: Some(1000),
                editor_line_height: Some(3),
                editor_letter_spacing: Some(999),
                ..Default::default()
            },
        )
        .unwrap();
    let resolved = store.appearance(Some(work.id)).unwrap();
    assert_eq!(resolved.editor_font_size, Some(30), "太大的字号夹到上限");
    assert_eq!(resolved.editor_line_height, Some(110), "太小的行距夹到下限");
    assert_eq!(resolved.editor_letter_spacing, Some(20));

    // ≤0 = 清掉（回默认那档），与"没设过"同义
    store
        .set_appearance(Some(work.id), &Appearance { editor_font_size: Some(0), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(Some(work.id)).unwrap().editor_font_size, None);

    // 每书覆盖：书的覆盖盖在全局上，全局那份不动
    store
        .set_appearance(None, &Appearance { editor_font_size: Some(24), ..Default::default() })
        .unwrap();
    store
        .set_appearance(Some(work.id), &Appearance { editor_font_size: Some(15), ..Default::default() })
        .unwrap();
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    assert_eq!(store.appearance(Some(work.id)).unwrap().editor_font_size, Some(15), "这一本覆盖了");
    assert_eq!(store.appearance(Some(other.id)).unwrap().editor_font_size, Some(24), "别的书跟全局");

    // 坏记录当没设过（旧版本写进去的荒谬值）
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, 0)",
            rusqlite::params![
                format!("work.{}.appearance", other.id),
                r#"{"editor_font_size":-5,"editor_line_height":"tall"}"#
            ],
        )
        .unwrap();
    let resolved = store.appearance(Some(other.id)).unwrap();
    assert_eq!(resolved.editor_font_size, Some(24), "坏值当没设过 → 回全局那份");
    assert_eq!(resolved.editor_line_height, None);
}

/// 叩问那三项偏好：默认（温柔 / 3 次 / 60 分钟）、"0 = 不打扰"与"负数 = 清掉这一层"分得清。
#[test]
fn asking_preferences_default_and_clear_on_their_own_terms() {
    let (_dir, mut store) = fresh();
    let defaults = store.appearance(None).unwrap();
    assert_eq!(defaults.question_tone, yanmo_core::model::QuestionTone::Warm, "默认温柔");
    assert_eq!(defaults.question_push_per_day, 3, "默认一天 3 次");
    assert_eq!(defaults.question_push_cooldown_minutes, 60, "默认冷却 60 分钟");

    // 0 是**正经取值**（＝不打扰），不是"清掉"——这条最容易写错
    store
        .set_appearance(
            None,
            &Appearance {
                question_push_per_day: Some(0),
                question_tone: Some("direct".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    let quiet = store.appearance(None).unwrap();
    assert_eq!(quiet.question_push_per_day, 0, "0 要原样存下来");
    assert_eq!(quiet.question_tone, yanmo_core::model::QuestionTone::Direct);

    // 负数 = 清掉这一层（回默认）；语气认不出的代码当场拒
    store
        .set_appearance(
            None,
            &Appearance {
                question_push_per_day: Some(-1),
                question_push_cooldown_minutes: Some(-1),
                question_tone: Some("auto".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    let cleared = store.appearance(None).unwrap();
    assert_eq!(
        (cleared.question_push_per_day, cleared.question_push_cooldown_minutes),
        (3, 60),
        "负数 = 回默认"
    );
    assert_eq!(cleared.question_tone, yanmo_core::model::QuestionTone::Warm);

    let err = store
        .set_appearance(
            None,
            &Appearance { question_tone: Some("shouty".to_string()), ..Default::default() },
        )
        .unwrap_err();
    assert_eq!(err.code(), "value.unknown_question_tone");
}

/// 第二栏露哪一块：**默认叩问、换过就记住、认不出的码当没设过**。
///
/// 它只决定那一栏先露哪一块，不进导出、不动正文——所以规矩跟语气 / 口径同一套：
/// 写进来不认识的当场拒，库里已有的坏码当没设过（回默认那一块）。
#[test]
fn the_second_column_remembers_which_block_was_showing() {
    let (_dir, mut store) = fresh();

    // 没设过 → 叩问（作者最常待的地方）
    assert_eq!(store.appearance(None).unwrap().aside_tab, SideTab::Flow);

    // 换到创作流：记住
    store
        .set_appearance(None, &Appearance { aside_tab: Some("creator".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().aside_tab, SideTab::Creator);

    // "auto" = 清掉这一层（回默认）；别的取值一律当场拒，且不落库
    store
        .set_appearance(None, &Appearance { aside_tab: Some("auto".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().aside_tab, SideTab::Flow);
    let err = store
        .set_appearance(None, &Appearance { aside_tab: Some("timeline".into()), ..Default::default() })
        .unwrap_err();
    assert_eq!(err.code(), "value.unknown_aside_tab");

    // 库里已有的坏码当没设过（跟坏 JSON、坏口径一条规矩）
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"aside_tab\":\"timeline\"}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().aside_tab, SideTab::Flow);
}

/// 界面配色：**默认浅色、换过就记住、认不出的码当没设过**，
/// 而且★**只走全局**——每书覆盖机制对它无效（"跟眼睛与房间光线有关，跟哪本书无关"）。
#[test]
fn the_theme_is_global_only_and_defaults_to_light() {
    use yanmo_core::model::Theme;

    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    // 没设过 → 浅色（与研墨一直以来的样子一致，不给没提过的人换底子）
    assert_eq!(store.appearance(None).unwrap().theme, Theme::Light);
    assert_eq!(store.appearance(Some(work.id)).unwrap().theme, Theme::Light);

    // 换到深色：记住；"跟随系统"也存得下（系统到底是深是浅由界面当场问，核心不存）
    store
        .set_appearance(None, &Appearance { theme: Some("dark".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().theme, Theme::Dark);
    store
        .set_appearance(None, &Appearance { theme: Some("system".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().theme, Theme::System);

    // ★ 只走全局：往某本书写主题**不该**改变这本书生效的那一份，
    //   也不该在每书记录里留一个永远没人读的死键
    store
        .set_appearance(None, &Appearance { theme: Some("dark".into()), ..Default::default() })
        .unwrap();
    store
        .set_appearance(Some(work.id), &Appearance { theme: Some("light".into()), ..Default::default() })
        .unwrap();
    assert_eq!(
        store.appearance(Some(work.id)).unwrap().theme,
        Theme::Dark,
        "配色不参与每书覆盖：这本书跟全局（深色）"
    );
    let book_raw = raw(&store, &format!("work.{}.appearance", work.id));
    assert!(
        book_raw.as_deref().is_none_or(|json| !json.contains("theme")),
        "每书的记录里不该落一个永远不被读的 theme 死键：{book_raw:?}"
    );

    // "auto" = 清掉这一层（回默认浅色），与命名 / 语气 / 第二栏同一条路
    store
        .set_appearance(None, &Appearance { theme: Some("auto".into()), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().theme, Theme::Light);

    // 认不出的码当场拒、且不落库；库里已有的坏码当没设过（跟坏 JSON 一条规矩）
    let err = store
        .set_appearance(None, &Appearance { theme: Some("sepia".into()), ..Default::default() })
        .unwrap_err();
    assert_eq!(err.code(), "value.unknown_theme");
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"theme\":\"sepia\"}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().theme, Theme::Light);
}

/// 用眼提醒：**默认开 + 宽阈值**、只走全局、两个阈值 ≤0 或超上限**当场拒**。
///
/// 它跟排版那三项**不**同一条路：排版是"纯观感"，坏值夹住就行；这两个数要拿去跟
/// "连着写了多久 / 今天写了多少"比，一个 0 或负数会让它每时每刻都够阈值——那正好把
/// "安静"整个反转了。所以写侧明确拒（界面把码渲染成一句人话），读侧仍按坏数据当没设过。
#[test]
fn the_break_reminder_is_on_by_default_and_refuses_a_useless_threshold() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    // 没设过 → 开 + 45 分钟 / 1500 字（宽阈值：只在停下点说一句，不打断写作）
    let defaults = store.appearance(None).unwrap();
    assert!(defaults.break_reminder, "默认开；嫌吵一键关");
    assert_eq!(defaults.break_minutes, 45);
    assert_eq!(defaults.break_words, 1500);

    // 往返：写进去读回来
    store
        .set_appearance(
            None,
            &Appearance {
                break_reminder: Some(false),
                break_minutes: Some(30),
                break_words: Some(800),
                ..Default::default()
            },
        )
        .unwrap();
    let saved = store.appearance(None).unwrap();
    assert!(!saved.break_reminder);
    assert_eq!(saved.break_minutes, 30);
    assert_eq!(saved.break_words, 800);

    // ≤0 与超出上限一律拒，**且不落库**（拒了之后库里那份还是刚才写进去的）
    for bad in [-3_i64, 0, 181] {
        let err = store
            .set_appearance(None, &Appearance { break_minutes: Some(bad), ..Default::default() })
            .unwrap_err();
        assert_eq!(err.code(), "break.minutes_out_of_range", "分钟 {bad} 该被拒");
    }
    for bad in [-3_i64, 0, 20_001] {
        let err = store
            .set_appearance(None, &Appearance { break_words: Some(bad), ..Default::default() })
            .unwrap_err();
        assert_eq!(err.code(), "break.words_out_of_range", "字数 {bad} 该被拒");
    }
    let kept = store.appearance(None).unwrap();
    assert_eq!((kept.break_minutes, kept.break_words), (30, 800), "被拒的那几次一个字都没改");

    // 上限本身是收的（180 分钟 / 20000 字）
    store
        .set_appearance(
            None,
            &Appearance { break_minutes: Some(180), break_words: Some(20_000), ..Default::default() },
        )
        .unwrap();
    let top = store.appearance(None).unwrap();
    assert_eq!((top.break_minutes, top.break_words), (180, 20_000));

    // 先把全局那一条打开（这样才分辨得出"这本书生效的"到底跟了谁）
    store
        .set_appearance(None, &Appearance { break_reminder: Some(true), ..Default::default() })
        .unwrap();

    // ★ 只走全局：往某本书写这三项**不该**改变这本书生效的那一份，也不落每书记录
    store
        .set_appearance(
            Some(work.id),
            &Appearance {
                break_reminder: Some(false),
                break_minutes: Some(5),
                break_words: Some(5),
                ..Default::default()
            },
        )
        .unwrap();
    let in_book = store.appearance(Some(work.id)).unwrap();
    assert!(in_book.break_reminder, "用眼提醒不参与每书覆盖");
    assert_eq!((in_book.break_minutes, in_book.break_words), (180, 20_000), "跟全局");
    let book_raw = raw(&store, &format!("work.{}.appearance", work.id));
    assert!(
        book_raw.as_deref().is_none_or(|json| !json.contains("break_")),
        "每书的记录里不该落用眼提醒的死键：{book_raw:?}"
    );

    // 库里已有的坏值当没设过（跟坏口径 / 坏配色一条规矩）：读侧不报错，回默认那一档
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"break_minutes\":0,\"break_words\":-5}', 0)",
            [],
        )
        .unwrap();
    let broken = store.appearance(None).unwrap();
    assert_eq!((broken.break_minutes, broken.break_words), (45, 1500), "坏值回默认");
}

/// 稿纸线型（实线 / 虚线 / 不显示）与排版三项是**同一条底线**：默认不显示、能记住、
/// 能每书覆盖、认不出的码当场拒，**而正文与导出一个字节都不许变**。
#[test]
fn the_rule_style_is_remembered_per_book_and_never_touches_the_text() {
    use yanmo_core::store::ExportFormat;
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    store.write_body(chapter, "第一段。\n第二段。").unwrap();
    let body_before = store.read_body(chapter).unwrap();
    let export_before: Vec<u8> = store
        .render_work(work.id, ExportFormat::Text)
        .unwrap()
        .iter()
        .flat_map(|file| file.content.clone())
        .collect();

    // 没设过 → 不显示（默认只有一处：核心的 `RuleStyle::default()`）
    assert_eq!(store.appearance(None).unwrap().editor_rule_style, RuleStyle::None);

    // 设全局：实线。只改这一项也要落库——`is_empty()` 漏了它就会被当成空记录删掉
    store
        .set_appearance(
            None,
            &Appearance { editor_rule_style: Some("solid".to_string()), ..Default::default() },
        )
        .unwrap();
    assert!(
        raw(&store, "appearance").is_some_and(|json| json.contains("solid")),
        "只改这一项也要落库（漏了它，作者选完重开就没了）"
    );

    // 每书覆盖：书的覆盖盖在全局上，全局那份不动
    store
        .set_appearance(
            Some(work.id),
            &Appearance { editor_rule_style: Some("dashed".to_string()), ..Default::default() },
        )
        .unwrap();
    let other = store.create_work(WorkKind::Novel, "另一本").unwrap();
    assert_eq!(
        store.appearance(Some(work.id)).unwrap().editor_rule_style,
        RuleStyle::Dashed,
        "这一本单独设过"
    );
    assert_eq!(
        store.appearance(Some(other.id)).unwrap().editor_rule_style,
        RuleStyle::Solid,
        "别的书跟全局"
    );
    assert_eq!(
        store.appearance(None).unwrap().editor_rule_style,
        RuleStyle::Solid,
        "书的覆盖不许动全局那一份"
    );

    // ★ 正文与导出：一个字节都不许变
    assert_eq!(store.read_body(chapter).unwrap(), body_before, "画线不许碰正文");
    let export_after: Vec<u8> = store
        .render_work(work.id, ExportFormat::Text)
        .unwrap()
        .iter()
        .flat_map(|file| file.content.clone())
        .collect();
    assert_eq!(export_after, export_before, "画线不许进导出");

    // 认不出的码**当场拒**（不是悄悄当没设过），被拒之后库里那份原样不动
    let err = store
        .set_appearance(
            None,
            &Appearance { editor_rule_style: Some("dotted".to_string()), ..Default::default() },
        )
        .unwrap_err();
    assert_eq!(err.code(), yanmo_core::error::codes::UNKNOWN_RULE_STYLE);
    assert_eq!(store.appearance(None).unwrap().editor_rule_style, RuleStyle::Solid);

    // `"auto"` = 清掉这一层（回默认：不显示）；书的覆盖清掉就是**回到继承全局**
    store
        .set_appearance(
            Some(work.id),
            &Appearance { editor_rule_style: Some("auto".to_string()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(store.appearance(Some(work.id)).unwrap().editor_rule_style, RuleStyle::Solid);
    store
        .set_appearance(
            None,
            &Appearance { editor_rule_style: Some("auto".to_string()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_rule_style, RuleStyle::None);

    // 坏记录当没设过（旧版本 / 手改 JSON 写进来的荒谬值）：读侧不报错，回全局那份
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, 0)",
            rusqlite::params![
                format!("work.{}.appearance", other.id),
                r#"{"editor_rule_style":"wavy"}"#
            ],
        )
        .unwrap();
    assert_eq!(store.appearance(Some(other.id)).unwrap().editor_rule_style, RuleStyle::None);
}
