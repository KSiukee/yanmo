//! 导出验收：**同样的内容永远渲染出同样的字节**（幂等），顺序稳定，结构用路径表达。
//!
//! 幂等这件事不是洁癖：导出结果要能放进 git、被人接手。只要渲染里混进时间戳或随机顺序，
//! 每次"再导一次"都会冒出一堆 diff，"稿子有没有变"就再也看不出来了。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{ExportFormat, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本两卷三章的书（零层级作品另有测试）。
fn novel(store: &mut Store) -> i64 {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let one = store.list_nodes(work.id).unwrap()[0].id;
    // 根卷默认是无名的（默认名不落库）；这里起个名，免得下面的路径断言被那件事带着走
    store.rename_node(one, "第一卷").unwrap();
    let two = store.create_node(work.id, None, NodeKind::Volume, "第二卷").unwrap();
    for (parent, title, body) in [
        (one, "第一章", "第一章的正文。"),
        (one, "第二章", "第二章的正文。"),
        (two, "第三章", ""),
    ] {
        let id = store.create_node(work.id, Some(parent), NodeKind::Chapter, title).unwrap();
        if !body.is_empty() {
            store.write_body(id, body).unwrap();
        }
    }
    work.id
}

fn paths(files: &[yanmo_core::store::RenderedFile]) -> Vec<String> {
    files.iter().map(|file| file.relative_path.clone()).collect()
}

fn find<'a>(files: &'a [yanmo_core::store::RenderedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.relative_path == path)
        .map(|file| std::str::from_utf8(&file.content).unwrap())
        .unwrap_or_else(|| panic!("没有这个文件：{path}；实际有 {:?}", paths(files)))
}

#[test]
fn text_export_keeps_the_structure_in_paths_and_the_order_stable() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);

    let files = store.render_work(work, ExportFormat::Text).unwrap();
    assert_eq!(
        paths(&files),
        vec![
            "001-第一卷/001-第一章.txt",
            "001-第一卷/002-第二章.txt",
            "002-第二卷/001-第三章.txt",
        ],
        "卷体现在目录上，章按阅读顺序编号"
    );
    assert_eq!(find(&files, "001-第一卷/001-第一章.txt"), "第一章的正文。\n");
    assert_eq!(
        find(&files, "002-第二卷/001-第三章.txt"),
        "",
        "没写过的章是一个空文件，不是没有文件"
    );
}

#[test]
fn rendering_twice_gives_byte_identical_results() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);

    for format in [ExportFormat::Text, ExportFormat::Json] {
        let first = store.render_work(work, format).unwrap();
        let second = store.render_work(work, format).unwrap();
        assert_eq!(first, second, "{:?} 两次渲染必须一模一样", format);
    }
}

#[test]
fn text_export_normalizes_newlines_and_the_trailing_break() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "短记").unwrap();
    let piece = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(piece, "第一段\r\n\r\n第二段\r\n\r\n\r\n").unwrap();

    let files = store.render_work(work.id, ExportFormat::Text).unwrap();
    assert_eq!(paths(&files), vec!["001-短记.txt"], "零层级作品就是一篇，不带目录");
    assert_eq!(
        find(&files, "001-短记.txt"),
        "第一段\n\n第二段\n",
        "CRLF 统一成 LF，末尾只留一个换行"
    );
}

#[test]
fn file_names_are_safe_and_still_ordered() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    for title in ["第一章：铜钱/玉佩", "第二章*迷雾?", "第三章"] {
        let id = store.create_node(work.id, Some(volume), NodeKind::Chapter, title).unwrap();
        store.write_body(id, "正文").unwrap();
    }

    let files = store.render_work(work.id, ExportFormat::Text).unwrap();
    let names = paths(&files);
    for name in &names {
        for bad in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            // 路径里的分隔符是**我们自己加的**，标题里的危险字符必须已经被换掉
            let title_part = name.rsplit('/').next().unwrap();
            assert!(!title_part.contains(bad), "{name} 里还有不能当文件名的字符：{bad}");
        }
    }
    assert_eq!(names.len(), 3);
    assert!(names[0] < names[1] && names[1] < names[2], "序号保证顺序稳定：{names:?}");
}

#[test]
fn json_export_carries_structure_and_bodies() {
    let (_dir, mut store) = fresh();
    let work = novel(&mut store);

    let files = store.render_work(work, ExportFormat::Json).unwrap();
    assert_eq!(paths(&files), vec!["work.json"]);
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&files[0].content).unwrap()).unwrap();
    assert_eq!(parsed["title"], "长夜");
    assert_eq!(parsed["kind"], "novel");

    let volumes = parsed["nodes"].as_array().unwrap();
    assert_eq!(volumes.len(), 2, "两卷");
    assert_eq!(volumes[0]["kind"], "volume");
    let first_chapter = &volumes[0]["children"][0];
    assert_eq!(first_chapter["kind"], "chapter");
    assert_eq!(first_chapter["title"], "第一章");
    assert_eq!(first_chapter["body"], "第一章的正文。\n");
    assert_eq!(
        volumes[1]["children"][0]["body"], "",
        "没写过的章给空串，字段仍在"
    );
}

#[test]
fn an_empty_book_still_exports_something_readable() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "刚开的坑").unwrap();
    let files = store.render_work(work.id, ExportFormat::Text).unwrap();
    assert_eq!(paths(&files), vec!["empty.txt"], "空书也要留下一个文件，不是空文件夹");
    assert_eq!(find(&files, "empty.txt"), "刚开的坑\n");
    assert!(
        files.iter().all(|file| file.relative_path.is_ascii()),
        "导出的文件名要语言无关——它会留在作者磁盘上，不该随界面语言变"
    );
}

#[test]
fn unnamed_containers_export_with_structural_names() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "").unwrap(); // 首次运行那本无名书
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    store.write_body(chapter, "正文。").unwrap();

    let files = store.render_work(work.id, ExportFormat::Text).unwrap();
    assert_eq!(
        paths(&files),
        vec!["001-volume/001-第1章.txt"],
        "没起名的**容器**退到结构标识；章仍按同层取号自动命名（那是作者可改的名字）"
    );
}
