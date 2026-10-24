//! 分类归属的验收：体裁与标签**存得下、读得回、归一得对**，
//! 而且**一个字都不许影响正文**。
//!
//! 三件事必须成立：
//! 1. 写进去的是**归一过的那一份**（去空白 / 去重 / 去空行 / 保序），超限如实拒；
//! 2. 老库（v19，没有这两列）打开时**自动补列**，且补出来是"没填过"而不是 NULL；
//! 3. 这一栏**不进正文、不进导出**：同一本书改体裁/标签前后，分章 txt 与集导出**逐字节一样**。

use yanmo_core::compile::{compile_collection, CollectionFormat};
use yanmo_core::model::WorkKind;
use yanmo_core::store::{ExportFormat, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn article(store: &mut Store, title: &str, body: &str) -> i64 {
    let work = store.create_work(WorkKind::Article, title).unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(root, body).unwrap();
    work.id
}

#[test]
fn a_new_work_starts_with_no_genre_and_no_tags() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    assert_eq!(work.genre, "");
    assert_eq!(work.tags, "", "空串＝没填过（不是 NULL，也不许预填一个体裁）");

    let read = store.get_work(work.id).unwrap();
    assert_eq!(read.genre, "");
    assert_eq!(read.tags, "");
}

#[test]
fn writing_classification_normalizes_and_reads_back_the_saved_copy() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "夜航").unwrap();

    let saved = store.set_work_classify(work.id, "  科幻 ", "太空歌剧\n\n  太空歌剧 \n群像\n").unwrap();
    assert_eq!(saved.genre, "科幻", "体裁去首尾空白");
    assert_eq!(saved.tags, "太空歌剧\n群像", "标签去空行、去重，保留先后");

    // 落定的是库里那一份：重新读一次、书架读一次，三处一模一样
    let again = store.work_classify(work.id).unwrap();
    assert_eq!(again, saved);
    let shelf = store.shelf().unwrap();
    let row = shelf.iter().find(|entry| entry.work.id == work.id).unwrap();
    assert_eq!(row.work.genre, "科幻");
    assert_eq!(row.work.tags, "太空歌剧\n群像");
}

#[test]
fn clearing_the_classification_is_a_real_write() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "夜航").unwrap();
    store.set_work_classify(work.id, "悬疑", "密室").unwrap();

    let cleared = store.set_work_classify(work.id, "   ", "").unwrap();
    assert_eq!(cleared.genre, "");
    assert_eq!(cleared.tags, "", "清空＝回到「没填过」，不是留下一个空格");
    assert_eq!(store.get_work(work.id).unwrap().genre, "");
}

#[test]
fn too_many_or_too_long_tags_are_refused_and_nothing_is_written() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "夜航").unwrap();
    store.set_work_classify(work.id, "科幻", "原来的标签").unwrap();

    let many: Vec<String> = (1..=20).map(|i| format!("标签{i}")).collect();
    assert!(
        store.set_work_classify(work.id, "科幻", &many.join("\n")).is_err(),
        "超过一打标签要如实拒，不许悄悄丢掉多的那些"
    );
    let long = "长".repeat(40);
    assert!(store.set_work_classify(work.id, "科幻", &long).is_err(), "单个标签超长也要拒");
    assert!(store.set_work_classify(work.id, &"体".repeat(40), "标签").is_err(), "体裁超长也要拒");

    // 被拒之后**库里一个字节没动**（还是上一次写进去的那一份）
    let kept = store.work_classify(work.id).unwrap();
    assert_eq!(kept.genre, "科幻");
    assert_eq!(kept.tags, "原来的标签");
}

#[test]
fn the_collection_read_model_carries_the_classification() {
    let (_dir, mut store) = fresh();
    let anthology = store.create_work(WorkKind::Collection, "自选").unwrap().id;
    let scifi = article(&mut store, "太空那篇", "正文一");
    let mystery = article(&mut store, "密室那篇", "正文二");
    store.set_work_classify(scifi, "科幻", "太空\n群像").unwrap();
    store.set_work_classify(mystery, "悬疑", "密室").unwrap();
    store.add_to_collection(anthology, scifi).unwrap();
    store.add_to_collection(anthology, mystery).unwrap();

    let view = store.collection_view(anthology).unwrap();
    let first = &view.entries[0];
    assert_eq!((first.genre.as_str(), first.tags.as_str()), ("科幻", "太空\n群像"));
    // 集里没归集的那一份读模型（散篇那张单子）也带同样的两格
    let loose = store.uncollected_articles().unwrap();
    assert!(loose.is_empty(), "两篇都归集了");

    let third = article(&mut store, "还散着", "正文三");
    store.set_work_classify(third, "", "没体裁有标签").unwrap();
    let loose = store.uncollected_articles().unwrap();
    assert_eq!(loose[0].genre, "");
    assert_eq!(loose[0].tags, "没体裁有标签");
}

#[test]
fn migration_twenty_is_idempotent_on_an_existing_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let work_id = {
        let mut store = Store::open(&path).unwrap();
        let work = store.create_work(WorkKind::Article, "夜航").unwrap();
        store.set_work_classify(work.id, "科幻", "太空").unwrap();
        // 伪造成"上一版"的库：迁移必须能从 19 走上来，且**不覆盖已有数据**
        store.conn().pragma_update(None, "user_version", 19).unwrap();
        work.id
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
    let kept = store.work_classify(work_id).unwrap();
    assert_eq!(kept.genre, "科幻", "补列不许把作者填过的东西弄丢");
    assert_eq!(kept.tags, "太空");
}

/// 最要紧的一条边界：分类归属是**作品的元数据**，不是正文的一部分。
///
/// 改它前后，分章 txt 与集导出**逐字节一样**——它进不了稿子，也不会进投稿包。
#[test]
fn classification_never_touches_the_manuscript_or_exports() {
    let (_dir, mut store) = fresh();
    let work = {
        let work = store.create_work(WorkKind::Article, "夜航").unwrap();
        let root = store.list_nodes(work.id).unwrap()[0].id;
        store.write_body(root, "第一段。\n\n第二段。").unwrap();
        work.id
    };
    let anthology = store.create_work(WorkKind::Collection, "自选").unwrap().id;
    store.add_to_collection(anthology, work).unwrap();

    let before_txt = store.render_work(work, ExportFormat::Text).unwrap();
    let before_json = store.render_work(work, ExportFormat::Json).unwrap();
    let before_docx = compile_collection(&store, anthology, CollectionFormat::Docx, None).unwrap();
    let before_body = {
        let root = store.list_nodes(work).unwrap()[0].id;
        store.read_body(root).unwrap()
    };

    store.set_work_classify(work, "科幻", "太空\n群像").unwrap();

    assert_eq!(store.render_work(work, ExportFormat::Text).unwrap(), before_txt);
    assert_eq!(store.render_work(work, ExportFormat::Json).unwrap(), before_json);
    assert_eq!(
        compile_collection(&store, anthology, CollectionFormat::Docx, None).unwrap(),
        before_docx
    );
    let root = store.list_nodes(work).unwrap()[0].id;
    assert_eq!(store.read_body(root).unwrap(), before_body, "正文一个字都不许动");

    // 反面证据：**导出里也看不到这一栏**（作者填的体裁不该自己跑进稿子）
    let plain = String::from_utf8(before_txt[0].content.clone()).unwrap();
    assert!(!plain.contains("科幻") && !plain.contains("太空"), "导出里不该有分类归属：{plain}");
}
