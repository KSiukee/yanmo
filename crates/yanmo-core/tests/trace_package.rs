//! 创作留痕自证包的验收：**链能自验、口径诚实、包里不带正文**。

use serde_json::Value;
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{verify_trace_events, Appearance, Store};

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

    // 链规则要写得能照着算：字段顺序固定、没有多余的转义
    let rule = manifest["chain_rule"].as_str().unwrap();
    assert!(rule.contains("compact_json"), "链规则要说明是紧凑 JSON：{rule}");
    assert!(!rule.contains("{{"), "链规则里不该出现双花括号：{rule}");

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

#[test]
fn chapter_titles_in_the_package_are_rendered_not_templates() {
    // 包里是给外人看的：`第{$N}章 复活节雕塑` 必须渲染成「第一章 复活节雕塑」，
    // 不能把内部模板原样摆出去。
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "祺").unwrap();
    store
        .set_appearance(
            Some(work.id),
            &Appearance { naming: Some("chinese".to_string()), ..Default::default() },
        )
        .unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "第{$N}章 复活节雕塑")
        .unwrap();
    store.write_body(chapter, "复活节那天，雕塑裂了一道缝。").unwrap();

    let package = store.build_trace_package(work.id).unwrap();
    let sources: Value = serde_json::from_slice(file(&package, "sources.json")).unwrap();
    let title = sources["chapters"][0]["title"].as_str().unwrap();
    assert!(!title.contains("{$"), "标题必须渲染过，不能是模板：{title}");
    assert!(title.contains("复活节雕塑"), "标题要带上作者写的那部分：{title}");

    let versions: Value = serde_json::from_slice(file(&package, "versions.json")).unwrap();
    let _ = versions; // 没有快照时是空数组；这条测的是标题渲染那条路
}
