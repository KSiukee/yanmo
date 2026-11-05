//! 创作留痕自证包的验收：**链能自验、口径诚实、包里不带正文**。

use serde_json::Value;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{verify_trace_events, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn file<'a>(package: &'a yanmo_core::store::TracePackage, name: &str) -> &'a [u8] {
    package
        .files
        .iter()
        .find(|file| file.name == name)
        .map(|file| file.bytes.as_slice())
        .unwrap_or_else(|| panic!("包里没有 {name}"))
}

#[test]
fn the_package_chains_every_event_and_keeps_the_boundaries_honest() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    store.write_body(chapter, "雨下了整夜。\n\n他把铜钱按在桌上。").unwrap();
    store.write_body(chapter, "雨下了整夜。\n\n他把铜钱按在桌上，又收了起来。").unwrap();

    let package = store.build_trace_package(work.id).unwrap();
    let names: Vec<&str> = package.files.iter().map(|file| file.name.as_str()).collect();
    for expected in ["events.jsonl", "sources.json", "versions.json", "README.txt", "manifest.json"] {
        assert!(names.contains(&expected), "包里该有 {expected}：{names:?}");
    }

    let jsonl = String::from_utf8(file(&package, "events.jsonl").to_vec()).unwrap();
    let (count, head) = verify_trace_events(&jsonl).expect("链必须自洽");
    assert_eq!(count, package.events);
    assert_eq!(head, package.head);

    // manifest 里的链头与逐文件哈希都要对得上
    let manifest: Value = serde_json::from_slice(file(&package, "manifest.json")).unwrap();
    assert_eq!(manifest["chain_head"], serde_json::json!(package.head));
    assert_eq!(manifest["event_count"], serde_json::json!(package.events));
    for (index, entry) in manifest["files"].as_array().unwrap().iter().enumerate() {
        let name = entry["name"].as_str().unwrap();
        let bytes = file(&package, name);
        assert_eq!(entry["bytes"], serde_json::json!(bytes.len()), "第 {} 个文件 {} 的字节数对不上", index, name);
    }

    // 口径必须写清「能证明 / 不能证明 / 不含正文」
    let readme = String::from_utf8(file(&package, "README.txt").to_vec()).unwrap();
    for phrase in ["能证明什么", "不能证明什么", "本机时间", "不包含正文"] {
        assert!(readme.contains(phrase), "README 少了「{phrase}」这一句");
    }

    // 包里不带正文：事件只带摘要与计数
    assert!(
        !jsonl.contains("铜钱按在桌上"),
        "事件流里不该出现正文原句（这是过程材料，不是交稿）"
    );

    // 每章一条来源账，且能看出「这一章的字从哪儿来」
    let sources: Value = serde_json::from_slice(file(&package, "sources.json")).unwrap();
    let chapters = sources["chapters"].as_array().unwrap();
    assert_eq!(chapters.len(), 1);
    assert!(chapters[0]["shares"].as_array().unwrap().iter().any(|share| {
        share["source"] == "unknown" || share["source"] == "typed"
    }));
}

#[test]
fn tampering_with_one_event_breaks_the_chain() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    store.write_body(chapter, "第一稿。").unwrap();

    let package = store.build_trace_package(work.id).unwrap();
    let jsonl = String::from_utf8(file(&package, "events.jsonl").to_vec()).unwrap();
    verify_trace_events(&jsonl).expect("原包必须自洽");

    // 改掉第一条里的时间戳（合法数字，但内容变了）：digest 立刻对不上
    let mut lines: Vec<String> = jsonl.lines().map(str::to_string).collect();
    lines[0] = lines[0].replacen("\"at\":", "\"at\":9", 1);
    let tampered = lines.join("\n") + "\n";
    let error = verify_trace_events(&tampered).expect_err("改过就必须验不过");
    assert!(error.contains("digest") || error.contains("prev"), "错误要说清在哪儿断的：{error}");
}
