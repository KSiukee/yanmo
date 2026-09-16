//! 编译管线的验收：**一份原稿 → 一种成品**，且同一次编译永远出同样的字节。
//!
//! 五条判据：
//! 1. 合并 txt 按阅读顺序拼，路径落在自己的预设子目录里；
//! 2. 投稿版 docx **真的是一个能打开的 ZIP**（用打包库读回来验，不是"看着像"）；
//! 3. 缩进走样式层的 `w:firstLineChars="200"`（两字符），正文里不塞缩进空格；
//! 4. 正文上限按段落卡住，**大纲与标题照旧给全**；
//! 5. 渲染两遍逐字节一样（换一种预设也不会互相覆盖）。
//!
//! 第 2 条为什么用外部的 zip 库来验：**验收不能自己验自己**——核心写 ZIP、
//! 核心再读 ZIP，写错了也可能"自洽"。这里用另一份实现把包打开，才算真的读过。

use std::io::Read;

use yanmo_core::compile::{compile, CompileOptions, Preset};
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{RenderedFile, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本书：一个卷 + 两章（各有正文与"一句话"）+ 作品简介 + 故事总纲。
fn book(store: &mut Store) -> i64 {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    store.set_work_summary(work.id, "一个关于等待的故事。").unwrap();
    store
        .set_work_storyline(work.id, "主线：他要把那盏灯等的那个人找回来。\n卖点：灯下坐的是谁，没人知道。")
        .unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    store.rename_node(volume, "第一卷 夜行").unwrap();

    let first = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第一章 门").unwrap();
    store.write_body(first, "他推开门，屋里没有人。\n\n茶还温着。").unwrap();
    store.set_node_summary(first, "他推开门，屋里没有人。").unwrap();

    let second = store.create_node(work.id, Some(volume), NodeKind::Chapter, "第二章 灯").unwrap();
    store.write_body(second, "灯亮了一夜。").unwrap();
    store.set_node_summary(second, "灯亮了一夜。").unwrap();
    work.id
}

fn text_of(file: &RenderedFile) -> String {
    String::from_utf8(file.content.clone()).unwrap()
}

fn open_docx(file: &RenderedFile) -> zip::ZipArchive<std::io::Cursor<Vec<u8>>> {
    zip::ZipArchive::new(std::io::Cursor::new(file.content.clone())).expect("docx 应当是一个能打开的 ZIP")
}

fn part(zip: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> String {
    let mut text = String::new();
    zip.by_name(name)
        .unwrap_or_else(|e| panic!("包里没有 {name}：{e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn merged_txt_follows_the_reading_order() {
    let (_dir, mut store) = fresh();
    let work_id = book(&mut store);

    let files = compile(&store, work_id, Preset::MergedTxt, &CompileOptions::defaults_for(Preset::MergedTxt)).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].relative_path.starts_with("merged/"), "落进自己的预设子目录");
    let text = text_of(&files[0]);

    assert!(text.starts_with("长夜"), "书名在最前：{text}");
    let order: Vec<usize> = ["长夜", "第一卷 夜行", "第一章 门", "他推开门", "第二章 灯", "灯亮了一夜"]
        .iter()
        .map(|needle| text.find(needle).unwrap_or_else(|| panic!("合并文本里少了 {needle}：{text}")))
        .collect();
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(order, sorted, "顺序必须是阅读顺序");
    assert!(text.ends_with('\n'), "末尾留一个换行");
}

#[test]
fn chapters_txt_keeps_one_file_per_chapter_inside_its_own_folder() {
    let (_dir, mut store) = fresh();
    let work_id = book(&mut store);

    let files =
        compile(&store, work_id, Preset::ChaptersTxt, &CompileOptions::defaults_for(Preset::ChaptersTxt)).unwrap();
    // 只有承载正文的章落成文件；卷只体现在路径里（与导出那条同一套口径）
    assert_eq!(files.len(), 2, "{:?}", files.iter().map(|f| &f.relative_path).collect::<Vec<_>>());
    assert!(files.iter().all(|file| file.relative_path.starts_with("chapters/")));
    assert!(files.iter().all(|file| file.relative_path.contains("第一卷 夜行/")));
    assert!(files.iter().any(|file| file.relative_path.ends_with("第一章 门.txt")));
}

#[test]
fn submission_docx_is_a_real_zip_with_the_parts_word_expects() {
    let (_dir, mut store) = fresh();
    let work_id = book(&mut store);

    let files = compile(
        &store,
        work_id,
        Preset::SubmissionDocx,
        &CompileOptions::defaults_for(Preset::SubmissionDocx),
    )
    .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "submission/长夜.docx");

    let mut zip = open_docx(&files[0]);
    for name in [
        "[Content_Types].xml",
        "_rels/.rels",
        "docProps/core.xml",
        "docProps/app.xml",
        "word/_rels/document.xml.rels",
        "word/styles.xml",
        "word/settings.xml",
        "word/document.xml",
    ] {
        assert!(!part(&mut zip, name).is_empty(), "{name} 不该是空的");
    }

    // 正文与大纲都在，且是**排好序**的
    let document = part(&mut zip, "word/document.xml");
    assert!(document.contains("长夜"));
    assert!(document.contains("一个关于等待的故事。"), "作品简介要进投稿包");
    assert!(document.contains("大纲"));
    // 故事总纲：**在大纲前面单独一节**，整本书那几段（编辑先看这一段）
    assert!(document.contains("故事总纲"), "投稿包要有总纲那一节：{document}");
    assert!(document.contains("他要把那盏灯等的那个人找回来。"), "总纲的原文要进投稿包");
    let storyline_at = document.find("故事总纲").unwrap();
    assert!(
        document[..storyline_at].contains("茶还温着"),
        "正文在总纲之前（与简介同一段排法）"
    );
    assert!(storyline_at < document.find("大纲").unwrap(), "总纲在大纲前面");
    assert!(document.contains("第一卷 夜行"));
    assert!(document.contains("第一章 门 —— 他推开门，屋里没有人。"), "大纲里要有每章那句话");
    let outline_at = document.find("大纲").unwrap();
    assert!(document[..outline_at].contains("茶还温着"), "正文在大纲之前");

    // 缩进在样式层：**字符单位为主、twips 兜底**（只给字符单位的话，不解析 *Chars 的
    // 阅读器会干脆不缩进；只给 twips 的话，改字号缩进就不跟着变了）
    let styles = part(&mut zip, "word/styles.xml");
    assert!(styles.contains("w:firstLineChars=\"200\""), "首行缩进两字符要写在样式里");
    assert!(styles.contains("w:firstLine=\"420\""), "twips 兜底也要给（420 = 五号下两字符）");
    assert!(styles.contains("宋体"), "字体名写在样式里（不内置字体文件）");

    // 标准包该有的东西：兼容模式设成 15、核心属性里有书名
    let settings = part(&mut zip, "word/settings.xml");
    assert!(settings.contains("compatibilityMode"), "要写兼容模式，否则 Word 按老版式打开");
    assert!(settings.contains("w:val=\"15\""), "兼容模式要设成 Word 2013+（15）");
    let core = part(&mut zip, "docProps/core.xml");
    assert!(core.contains("<dc:title>长夜</dc:title>"), "核心属性里要有书名：{core}");
    assert!(!core.contains("dcterms:created"), "不许写时间戳（会破坏编译幂等）");
    // 正文段落里不许塞缩进空格：段首就是文字
    assert!(document.contains("<w:t xml:space=\"preserve\">他推开门，屋里没有人。</w:t>"));
}

#[test]
fn the_submission_body_stops_at_the_limit_but_the_outline_stays_complete() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    // 十段，每段 100 个汉字：上限 300 字时只该装下前三段
    let paragraph = "字".repeat(100);
    for index in 0..10 {
        let chapter = store
            .create_node(work.id, Some(volume), NodeKind::Chapter, &format!("第{}章", index + 1))
            .unwrap();
        store.write_body(chapter, &paragraph).unwrap();
        store.set_node_summary(chapter, &format!("第{}章的一句话", index + 1)).unwrap();
    }

    let options = CompileOptions { body_limit: Some(300), with_outline: true };
    let files = compile(&store, work.id, Preset::SubmissionDocx, &options).unwrap();
    let mut zip = open_docx(&files[0]);
    let document = part(&mut zip, "word/document.xml");

    // 正文被卡住：装下的段落数不超过 3 段（每段 100 字）
    assert_eq!(document.matches(&paragraph).count(), 3, "超限的正文不许装进来");
    // 但标题与大纲照旧给全：十章都在
    for index in 1..=10 {
        assert!(document.contains(&format!("第{index}章")), "第 {index} 章的标题不该被上限吃掉");
        assert!(document.contains(&format!("第{index}章的一句话")), "大纲要覆盖全书");
    }
}

#[test]
fn compiling_twice_gives_the_same_bytes() {
    let (_dir, mut store) = fresh();
    let work_id = book(&mut store);
    let options = CompileOptions::defaults_for(Preset::SubmissionDocx);

    let first = compile(&store, work_id, Preset::SubmissionDocx, &options).unwrap();
    let second = compile(&store, work_id, Preset::SubmissionDocx, &options).unwrap();
    assert_eq!(first, second, "同样的原稿、同样的参数，编译出来的字节必须一样");
}

#[test]
fn titles_that_contain_xml_specials_do_not_break_the_document() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "A & B <C>").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    store.rename_node(volume, "卷一 & <起点>").unwrap();
    let chapter = store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "第一章 \"引号\" & <门>")
        .unwrap();
    store.write_body(chapter, "正文里也有 & 与 <tag>。").unwrap();

    let files = compile(
        &store,
        work.id,
        Preset::SubmissionDocx,
        &CompileOptions::defaults_for(Preset::SubmissionDocx),
    )
    .unwrap();
    let mut zip = open_docx(&files[0]);
    let document = part(&mut zip, "word/document.xml");
    assert!(document.contains("A &amp; B &lt;C&gt;"));
    assert!(document.contains("卷一 &amp; &lt;起点&gt;"));
    assert!(document.contains("第一章 &quot;引号&quot; &amp; &lt;门&gt;"));
    assert!(document.contains("正文里也有 &amp; 与 &lt;tag&gt;。"));
    assert!(!document.contains("<tag>"), "原样的尖括号不许进 XML");
}

#[test]
fn an_unknown_preset_is_refused() {
    let error = Preset::parse("epub").unwrap_err();
    assert_eq!(error.code(), "value.unknown_compile_preset");
    for preset in Preset::ALL {
        assert_eq!(Preset::parse(preset.as_str()).unwrap(), *preset);
    }
}
