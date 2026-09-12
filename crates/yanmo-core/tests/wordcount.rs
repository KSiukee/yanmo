//! 字数口径与作品语言的验收——**口径是产品的硬承诺**，所以在这里钉死。
//!
//! 分两层：
//! - 纯函数那一层（全半角 / 零宽 / emoji / 缩进 / 假名谚文）在 `src/text.rs` 的单测里；
//! - 这里管**落地这一层**：写进去再读回来是不是同一套数、三个口径是不是都在、
//!   作品语言改了之后默认口径跟不跟着走、老库升上来语言是不是中文。

use yanmo_core::model::{WorkKind, WorkLanguage};
use yanmo_core::store::Store;
use yanmo_core::text::WordCaliber;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn three_calibers_come_back_from_a_real_write_and_read() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;

    // 中文 + 标点 + 英文词 + 全角字母数字 + emoji + 零宽 + 缩进空白
    let body = "　　你好，世界。\n这是 Rust 入门 Ａ１\u{200b}🙂";
    let written = store.write_body(node, body).unwrap();

    assert_eq!(written.chars_no_punct, 14, "汉字 8 + Rust 4 字母 + 全角 Ａ１ 2；标点/emoji/零宽/缩进都不算");
    assert_eq!(written.word_count, 10, "汉字 8 逐字 + Rust 一串 + Ａ１ 一串");
    assert_eq!(written.char_count, written.chars_no_punct + 3, "多出来的正是 2 个标点 + 1 个 emoji");

    // 读回来必须是同一套数（读的是现算，不读库里的旧值）
    let (read_back, stats) = store.read_body_with_stats(node).unwrap();
    assert_eq!(read_back, body);
    assert_eq!(stats, written, "写入与读回的口径必须一致");
}

#[test]
fn caliber_counts_dispatch_to_the_same_three_numbers() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;
    let body = "你好，world。";
    let stats = store.write_body(node, body).unwrap();

    assert_eq!(WordCaliber::Chars.count(body), stats.char_count);
    assert_eq!(WordCaliber::CharsNoPunct.count(body), stats.chars_no_punct);
    assert_eq!(WordCaliber::Words.count(body), stats.word_count);
}

#[test]
fn kana_and_hangul_are_counted_per_char_not_as_one_word() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;

    let kana = store.write_body(node, "こんにちは、世界").unwrap();
    assert_eq!(kana.chars_no_punct, 7, "假名 5 + 汉字 2");
    assert_eq!(kana.word_count, 7, "假名逐字——修好之前这里只会是 3（一个假名串 + 两个汉字）");

    let hangul = store.write_body(node, "안녕하세요").unwrap();
    assert_eq!(hangul.word_count, 5);
}

#[test]
fn work_language_defaults_to_chinese_and_can_be_changed() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    assert_eq!(work.language, WorkLanguage::Zh, "新书默认中文");

    store.set_work_language(work.id, WorkLanguage::En).unwrap();
    let reread = store.list_works().unwrap();
    assert_eq!(reread[0].language, WorkLanguage::En, "改了就落库");
    assert_eq!(store.get_work(work.id).unwrap().language, WorkLanguage::En);
}

#[test]
fn default_caliber_follows_the_work_language() {
    assert_eq!(WorkLanguage::Zh.default_caliber(), WordCaliber::Chars);
    assert_eq!(WorkLanguage::Ja.default_caliber(), WordCaliber::Chars);
    assert_eq!(WorkLanguage::En.default_caliber(), WordCaliber::Words);
    for language in [WorkLanguage::Zh, WorkLanguage::En, WorkLanguage::Ja] {
        assert_eq!(WorkLanguage::parse(language.as_str()).unwrap(), language);
    }
    assert!(WorkLanguage::parse("fr").is_err(), "不认识的语言要报错，不猜");
}

#[test]
fn a_v3_library_is_upgraded_and_gets_chinese_as_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");

    // 造一个"v3 时代的库"：v1/v2 的结构 + v3 那一步实际做的改动，但没有 works.language
    {
        let conn = yanmo_core::db::open(&path).unwrap();
        for step in yanmo_core::db::migrations::MIGRATIONS[0].steps {
            conn.execute(step, []).unwrap();
        }
        for step in yanmo_core::db::migrations::MIGRATIONS[1].steps {
            conn.execute(step, []).unwrap();
        }
        for step in yanmo_core::db::migrations::MIGRATIONS[2]
            .prepare
            .expect("v3 的改动在 prepare 里")(&conn)
            .unwrap()
        {
            conn.execute(&step, []).unwrap();
        }
        conn.execute(
            "INSERT INTO works(id, kind, title, created_at, updated_at, opened_at)
             VALUES(1, 'novel', '旧稿', 1, 1, 1)",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
    }

    let store = Store::open(&path).unwrap();
    let work = store.get_work(1).unwrap();
    assert_eq!(work.language, WorkLanguage::Zh, "存量书一律当中文，不猜");
    let version: i64 = store
        .conn()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, i64::from(yanmo_core::db::migrations::schema_version()));
}
