//! 正文来源留痕验收：**打字与整段粘贴在留痕里必须长得不一样**。
//!
//! 这一组只盯一件事：落盘之后，"这一章的字从哪来"能不能如实回答。
//! 界面上那一行分档字数（[`SourceBreakdown`]）与 `op_log` 里那一笔的来源，
//! 都必须对得上作者刚才做的事——粘贴不许冒充键入，没观测到的写入不许冒充任何来源。

use yanmo_core::error_codes::codes;
use yanmo_core::model::{EditOrigin, FragmentKind, WorkKind, WriteOrigin};
use yanmo_core::store::{NewFragment, SourceBreakdown, Store};
use yanmo_core::text;

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn node_of(store: &mut Store) -> i64 {
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    store.list_nodes(work.id).unwrap()[0].id
}

fn last_write_payload(store: &Store) -> serde_json::Value {
    let raw: String = store
        .conn()
        .query_row(
            "SELECT payload FROM op_log WHERE entity = 'node_contents' AND op = 'write'
             ORDER BY seq DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn chars_of(breakdown: &SourceBreakdown, origin: EditOrigin) -> i64 {
    breakdown.chars_of(origin)
}

fn totals(breakdown: &SourceBreakdown) -> (i64, i64, i64) {
    breakdown.shares.iter().fold((0, 0, 0), |acc, share| {
        (
            acc.0 + share.char_count,
            acc.1 + share.chars_no_punct,
            acc.2 + share.word_count,
        )
    })
}

#[test]
fn a_paste_does_not_look_like_typing_in_the_ledger_or_the_log() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    store.write_body_counted(node, "开头一句。结尾一句。", 480, WriteOrigin::typed()).unwrap();

    let pasted = "粘进来的一大段，来源不明。";
    let body = format!("开头一句。{pasted}结尾一句。");
    let outcome = store
        .write_body_counted(node, &body, 480, WriteOrigin::pasted(42))
        .unwrap();

    assert_eq!(chars_of(&outcome.sources, EditOrigin::Pasted), text::count_chars(pasted));
    assert_eq!(
        chars_of(&outcome.sources, EditOrigin::Typed),
        text::count_chars(&body) - text::count_chars(pasted)
    );
    assert_eq!(totals(&outcome.sources), (outcome.counts.char_count, outcome.counts.chars_no_punct, outcome.counts.word_count));

    let payload = last_write_payload(&store);
    assert_eq!(payload["origin"], "pasted", "这一笔留痕必须写着粘贴");
    assert_eq!(payload["clipboard_chars"], 42, "壳观测到的剪贴板字面量要如实记下");
}

#[test]
fn typing_inside_a_pasted_sentence_splits_the_ledger_at_the_character() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    let pasted = "这是一整段粘贴进来的话。";
    store.write_body_counted(node, pasted, 480, WriteOrigin::pasted(14)).unwrap();

    let typed = "又补了三个字";
    let body = pasted.replacen("整段", &format!("整段{typed}"), 1);
    let outcome = store.write_body_counted(node, &body, 480, WriteOrigin::typed()).unwrap();

    assert_eq!(chars_of(&outcome.sources, EditOrigin::Typed), text::count_chars(typed));
    assert_eq!(chars_of(&outcome.sources, EditOrigin::Pasted), text::count_chars(pasted));
    assert_eq!(last_write_payload(&store)["origin"], "typed");
}

#[test]
fn an_unobserved_write_is_recorded_as_unknown_not_as_typed() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    // 没有观测者的写入（恢复 / 导入 / 命令行）：一句话都没报，就只能记"未记录"
    store.write_body(node, "这段字是谁写的，核心不知道。").unwrap();
    let breakdown = store.source_breakdown(node).unwrap();
    assert_eq!(breakdown.shares.len(), 1);
    assert_eq!(breakdown.shares[0].source, "unknown");
    assert_eq!(breakdown.shares[0].char_count, text::count_chars("这段字是谁写的，核心不知道。"));
}

#[test]
fn a_legacy_chapter_keeps_its_unknown_text_while_new_typing_is_traced() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    store.write_body(node, "老稿子的第一句。").unwrap();
    let body = "老稿子的第一句。今天补的一句。";
    let outcome = store
        .write_body_counted(node, body, 480, WriteOrigin::typed())
        .unwrap();
    assert_eq!(chars_of(&outcome.sources, EditOrigin::Unknown), text::count_chars("老稿子的第一句。"));
    assert_eq!(chars_of(&outcome.sources, EditOrigin::Typed), text::count_chars("今天补的一句。"));
    assert_eq!(outcome.counts.char_count, text::count_chars(body));
}

#[test]
fn a_module_delivery_without_a_stamp_is_refused_before_anything_is_written() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    store.write_body(node, "原来的正文。").unwrap();
    let before = store.source_breakdown(node).unwrap();

    let bare = store.write_body_counted(node, "模块想悄悄塞进来的整章。", 480, WriteOrigin::of(EditOrigin::Module));
    assert_eq!(bare.unwrap_err().code(), codes::SOURCE_STAMP_INVALID);
    assert_eq!(store.read_body(node).unwrap(), "原来的正文。", "拒收必须发生在写库之前");
    assert_eq!(store.source_breakdown(node).unwrap(), before, "拒收不得动来源账");

    let stamped = WriteOrigin::module("mod.plot-map", "mdl-2026-10", 900).unwrap();
    let outcome = store.write_body_counted(node, "模块交付的整章。", 480, stamped).unwrap();
    // 句号与旧正文里那个"。"逐字相同：它算"没动过的字"，仍记原来的来源（未记录）。
    // 模块真正写进来的那 7 个字必须记成模块——这正是"精确标注"要看的那一格。
    assert_eq!(
        chars_of(&outcome.sources, EditOrigin::Module),
        text::count_chars("模块交付的整章。") - 1
    );
    assert_eq!(
        chars_of(&outcome.sources, EditOrigin::Module) + chars_of(&outcome.sources, EditOrigin::Unknown),
        outcome.counts.char_count
    );
    let payload = last_write_payload(&store);
    assert_eq!(payload["origin"], "module");
    assert_eq!(payload["module_id"], "mod.plot-map");
    assert_eq!(payload["model"], "mdl-2026-10");
}

#[test]
fn a_stamp_on_a_typed_save_is_refused_too() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    let mut forged = WriteOrigin::typed();
    forged.stamp = Some(yanmo_core::model::DeliveryStamp {
        module_id: "mod.plot-map".into(),
        model: "mdl-2026-10".into(),
    });
    assert_eq!(
        store.write_body_counted(node, "冒充模块交付的一段字。", 480, forged).unwrap_err().code(),
        codes::SOURCE_STAMP_INVALID
    );
    assert_eq!(store.read_body(node).unwrap(), "");
}

#[test]
fn the_same_body_written_again_does_not_rewrite_the_ledger() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    let body = "一字不动的正文。";
    store.write_body_counted(node, body, 480, WriteOrigin::typed()).unwrap();
    let before = store.source_breakdown(node).unwrap();
    let writes: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM op_log WHERE entity = 'node_contents'", [], |r| r.get(0))
        .unwrap();

    // 自动保存会反复提交同一份内容：来源不许被"最后一次调用"改写
    let again = store.write_body_counted(node, body, 480, WriteOrigin::pasted(99)).unwrap();
    assert_eq!(again.sources, before);
    assert_eq!(chars_of(&again.sources, EditOrigin::Pasted), 0);
    let after: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM op_log WHERE entity = 'node_contents'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, writes, "内容没变就不该多一条留痕");
}

#[test]
fn splitting_a_chapter_splits_the_ledger_with_it() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "随笔").unwrap();
    let node = store.list_nodes(work.id).unwrap()[0].id;
    let head = "前半段是打出来的。";
    let tail = "后半段是粘进来的。";
    let body = format!("{head}\n\n{tail}");
    store.write_body_counted(node, head, 480, WriteOrigin::typed()).unwrap();
    store.write_body_counted(node, &body, 480, WriteOrigin::pasted(8)).unwrap();

    let receipt = store.split_chapter(node, 1, "").unwrap();
    let head_breakdown = store.source_breakdown(receipt.old_node_id).unwrap();
    let tail_breakdown = store.source_breakdown(receipt.new_node_id).unwrap();
    assert_eq!(chars_of(&head_breakdown, EditOrigin::Typed), text::count_chars(head));
    assert_eq!(chars_of(&tail_breakdown, EditOrigin::Pasted), text::count_chars(tail));
    assert_eq!(totals(&head_breakdown).0, text::count_chars(head));
    assert_eq!(totals(&tail_breakdown).0, text::count_chars(tail));
}

#[test]
fn adopting_an_external_mirror_edit_keeps_the_untouched_paragraphs() {
    let (_dir, mut store) = fresh();
    let node = node_of(&mut store);
    let original = "开头这一段是打出来的。\n\n第二段也是打出来的。";
    store.write_body_counted(node, original, 480, WriteOrigin::typed()).unwrap();

    let edited = "开头这一段是打出来的。\n\n第二段被外面改过了。";
    assert!(store.adopt_mirror_body(node, edited).unwrap());

    let breakdown = store.source_breakdown(node).unwrap();
    // 第一段整段没动；第二段的公共前后缀（「第二段」「。」）也没动——
    // 只有中间"被外面改过了"这 6 个字是外部改动（没有观测者 → 未记录）。
    assert_eq!(
        chars_of(&breakdown, EditOrigin::Typed),
        text::count_chars("开头这一段是打出来的。第二段。")
    );
    assert_eq!(chars_of(&breakdown, EditOrigin::Unknown), text::count_chars("被外面改过了"));
    assert_eq!(totals(&breakdown).0, text::count_chars(edited));
}

#[test]
fn a_voice_spark_landed_as_a_scene_card_keeps_its_source() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "书").unwrap();
    let chapter = store.ensure_editor_target().unwrap().node_id;
    let body = "口述记下的一场雨。";
    let spark = store
        .create_fragment(
            &NewFragment {
                work_id: work.id,
                kind: FragmentKind::Idea,
                body: body.to_string(),
                source: "voice".to_string(),
                anchors: vec![],
            },
            "test",
        )
        .unwrap();

    let receipt = store.adopt_spark(spark, "scene", Some(chapter), "雨夜", "test").unwrap();
    let scene = receipt.scene_id.expect("该新建一张场景卡");
    let breakdown = store.source_breakdown(scene).unwrap();
    assert_eq!(
        chars_of(&breakdown, EditOrigin::Voice),
        text::count_chars(body),
        "口述记下的字落进场景卡，来源仍应是口述——不是未记录，更不是键入"
    );
    assert_eq!(breakdown.shares.len(), 1);
}
