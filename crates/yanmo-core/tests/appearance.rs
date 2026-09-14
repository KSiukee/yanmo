//! 外观偏好验收：**全局打底 + 每书可选覆盖**，只存改过的项，坏记录当没设过。
//!
//! 四条判据：
//! 1. 没设过 → 默认值（默认只有一处，读的时候才落）；
//! 2. 写进去的只留改过的项（稀疏），全改回"没设过"就把键清掉；
//! 3. 书的覆盖 **只覆盖它真设过的项**，其余继承全局；
//! 4. 坏 JSON 不报错、当没设过——界面不该被一条坏记录卡住。

use yanmo_core::model::{NodeKind, WorkKind};
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
