//! 作者信息的验收：稀疏合并、坏记录当没设过、**敏感项默认不写进稿件**、留痕不抄取值。
//!
//! 最后一条是这一份里最要紧的：身份证号与银行账号会存在库里（作者自己机器上，
//! 为了"某家刊物要时不用再翻"），但**变更日志里不该跟着抄一份**——
//! 日志是给排查看的，不需要为了排查把卡号留在里面。

use yanmo_core::error_codes::codes;
use yanmo_core::model::Author;
use yanmo_core::store::Store;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn full() -> Author {
    Author {
        pen_name: Some("南岸".into()),
        real_name: Some("李明".into()),
        phone: Some("13800000000".into()),
        email: Some("nan@example.com".into()),
        address: Some("某省某市某路 1 号".into()),
        postcode: Some("100000".into()),
        bio: Some("写过一些短篇。\n发过几篇散文。".into()),
        id_number: Some("110101199001010011".into()),
        bank: Some("工商银行 6222020000000000".into()),
        attach: Some(true),
        with_sensitive: None,
    }
}

#[test]
fn nothing_set_reads_as_an_empty_author() {
    let (_dir, store) = fresh();
    let author = store.author().unwrap();
    assert_eq!(author.pen_name, "");
    assert!(!author.attach, "默认不附");
    assert!(!author.with_sensitive, "默认不带敏感项");
    assert_eq!(store.author_block().unwrap(), None);
}

#[test]
fn only_what_was_given_is_written() {
    let (_dir, mut store) = fresh();
    store.set_author(&Author { pen_name: Some("南岸".into()), ..Author::default() }).unwrap();
    store.set_author(&Author { phone: Some("13800000000".into()), ..Author::default() }).unwrap();
    let author = store.author().unwrap();
    assert_eq!(author.pen_name, "南岸", "没再传的栏要保持原样");
    assert_eq!(author.phone, "13800000000");
}

#[test]
fn clearing_one_field_is_explicit() {
    let (_dir, mut store) = fresh();
    store.set_author(&full()).unwrap();
    store.set_author(&Author { bank: Some(String::new()), ..Author::default() }).unwrap();
    let author = store.author().unwrap();
    assert_eq!(author.bank, "", "Some(\"\") = 清空那一栏");
    assert_eq!(author.pen_name, "南岸", "别的栏不受影响");
}

#[test]
fn a_broken_record_is_treated_as_unset() {
    let (_dir, store) = fresh();
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('author', '{{{ 不是 JSON', 1)",
            [],
        )
        .unwrap();
    let author = store.author().unwrap();
    assert_eq!(author.pen_name, "", "坏记录当没设过，绝不拿它挡住界面");
}

#[test]
fn reset_puts_it_back_to_unset() {
    let (_dir, mut store) = fresh();
    store.set_author(&full()).unwrap();
    let cleared = store.reset_author().unwrap();
    assert_eq!(cleared.pen_name, "");
    assert_eq!(store.author_block().unwrap(), None);
}

#[test]
fn the_sensitive_two_stay_out_of_the_document_unless_asked() {
    let (_dir, mut store) = fresh();
    // `full()` 里勾了"附"、没勾敏感 → 简介在，身份证与卡号不在
    store.set_author(&full()).unwrap();
    let block = store.author_block().unwrap().unwrap();
    assert_eq!(block.pen_name, "南岸");
    assert_eq!(block.real_name, "李明");
    assert!(block.id_number.is_empty() && block.bank.is_empty(), "敏感两项默认不写进稿子");

    // 把"附"关掉：一个字节都不写
    store
        .set_author(&Author { attach: Some(false), ..Author::default() })
        .unwrap();
    assert_eq!(store.author_block().unwrap(), None);

    // 两样都勾上：才带上
    store
        .set_author(&Author { attach: Some(true), with_sensitive: Some(true), ..Author::default() })
        .unwrap();
    let block = store.author_block().unwrap().unwrap();
    assert_eq!(block.id_number, "110101199001010011");
    assert!(block.bank.contains("6222020000000000"));
}

#[test]
fn a_too_long_field_is_refused_by_name() {
    let (_dir, mut store) = fresh();
    let err = store
        .set_author(&Author { phone: Some("1".repeat(300)), ..Author::default() })
        .unwrap_err();
    assert_eq!(err.code(), codes::AUTHOR_TOO_LONG);
    assert_eq!(store.author().unwrap().phone, "", "被拒绝时不该写进去");
}

#[test]
fn the_change_log_says_which_fields_moved_but_not_their_values() {
    let (_dir, mut store) = fresh();
    store.set_author(&full()).unwrap();
    let payload: String = store
        .conn()
        .query_row(
            "SELECT payload FROM op_log WHERE entity = 'settings' AND op = 'set_author'
              ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(payload.contains("bank"), "要说清动过哪几栏：{payload}");
    assert!(!payload.contains("6222020000000000"), "取值（尤其卡号）不许抄进日志：{payload}");
    assert!(!payload.contains("110101199001010011"), "身份证号同样不许：{payload}");
}
