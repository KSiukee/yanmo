//! 模块读核心数据的门卫验收：**授予才放行、冻结全停、只读**。
//!
//! 不测八个区域各自的业务语义（那些各有自己的验收），只测门卫与真数据接得上：
//! 给了哪一块就能读哪一块，没给的连方法都不该返回数据。

use serde_json::{json, Value};
use yanmo_core::error_codes::codes;
use yanmo_core::model::{FragmentKind, NodeKind, WorkKind};
use yanmo_core::store::{NewFragment, Store};
use yanmo_core::Error;
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

const ID: &str = "app.example.map";

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn manifest() -> ModuleManifest {
    let value: Value = json!({
        "id": ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "剧情地图" },
        "description": { "zh-Hans": "按章节看地点怎么走。" },
        "author": { "name": "Example Studio" },
        "version": "1.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": {
            "read": ["works"],
            "own_data": true,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    });
    let mut manifest = ModuleManifest::from_json(&value.to_string()).unwrap();
    // 这份清单声明"八块都可读"；测试用授予决定实际给哪几块
    manifest.capabilities.read = CoreDataArea::ALL.to_vec();
    manifest
}

fn grant(read: &[CoreDataArea]) -> ModuleCapabilities {
    ModuleCapabilities { read: read.to_vec(), own_data: true, ..ModuleCapabilities::none() }
}

fn all_caps() -> ModuleCapabilities {
    grant(&CoreDataArea::ALL)
}

/// 一本书 + 一章写了正文的章。
fn seeded(store: &mut Store) -> (i64, i64) {
    let work = store.create_work(WorkKind::Novel, "题本").unwrap();
    let chapter = store.create_node(work.id, None, NodeKind::Chapter, "第一章").unwrap();
    store.write_body(chapter, "第一章的正文。").unwrap();
    (work.id, chapter)
}

fn param(error: &Error, name: &str) -> String {
    error
        .params()
        .into_iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("缺少参数：{name}"))
}

#[test]
fn a_reader_only_sees_what_was_granted() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.install_module(&manifest(), &grant(&[CoreDataArea::Works])).unwrap();
    let reader = store.module_reader(ID).unwrap();
    assert_eq!(reader.module_id(), ID);
    assert_eq!(reader.works().unwrap().len(), 1, "授予了作品就能列");

    // 其余七块都没授予：逐块拒，且说清是哪一块
    let denied = [
        ("tree", reader.tree(work).unwrap_err()),
        ("body", reader.body(chapter).unwrap_err()),
        ("outline", reader.outline(work).unwrap_err()),
        ("fragments", reader.fragments(work).unwrap_err()),
        ("entities", reader.entities(work).unwrap_err()),
        ("scenes", reader.scenes(work).unwrap_err()),
        ("foreshadows", reader.foreshadows(work).unwrap_err()),
    ];
    for (capability, error) in denied {
        assert_eq!(error.code(), codes::MODULE_CAPABILITY_DENIED, "capability: {capability}");
        assert_eq!(param(&error, "capability"), capability);
    }
}

#[test]
fn granting_one_more_area_opens_only_that_door() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.install_module(&manifest(), &grant(&[CoreDataArea::Works])).unwrap();
    store.grant_module(ID, &grant(&[CoreDataArea::Tree])).unwrap();
    let reader = store.module_reader(ID).unwrap();
    let tree = reader.tree(work).unwrap();
    assert!(tree.iter().any(|node| node.id == chapter));
    assert_eq!(
        reader.works().unwrap_err().code(),
        codes::MODULE_CAPABILITY_DENIED,
        "换授予是替换，不是叠加"
    );
    assert_eq!(reader.body(chapter).unwrap_err().code(), codes::MODULE_CAPABILITY_DENIED);
}

#[test]
fn a_frozen_module_reads_nothing() {
    let (_dir, mut store) = fresh();
    let (work, _chapter) = seeded(&mut store);
    store.install_module(&manifest(), &all_caps()).unwrap();
    store.set_module_enabled(ID, false).unwrap();
    let reader = store.module_reader(ID).unwrap();
    let error = reader.works().unwrap_err();
    assert_eq!(error.code(), codes::MODULE_DISABLED);
    assert_eq!(param(&error, "module_id"), ID);
    assert_eq!(reader.tree(work).unwrap_err().code(), codes::MODULE_DISABLED);
}

#[test]
fn the_facade_reads_the_books_real_data() {
    let (_dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    let fragment = store
        .create_fragment(
            &NewFragment {
                work_id: work,
                kind: FragmentKind::Idea,
                body: "一个念头".to_string(),
                source: "typed".to_string(),
                anchors: vec![format!("chapter:{chapter}")],
            },
            "test",
        )
        .unwrap();
    store.install_module(&manifest(), &all_caps()).unwrap();
    let reader = store.module_reader(ID).unwrap();

    let works = reader.works().unwrap();
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].work.title, "题本");

    let tree = reader.tree(work).unwrap();
    let chapter_row = tree.iter().find(|node| node.id == chapter).expect("目录里应当有这一章");
    assert!(chapter_row.has_body);
    assert!(tree.iter().any(|node| node.id != chapter), "还有卷 / 根节点");

    assert_eq!(reader.body(chapter).unwrap(), "第一章的正文。");

    let outline = reader.outline(work).unwrap();
    assert_eq!(outline.len(), tree.len(), "大纲与目录同序同量");
    let outline_row = outline.iter().find(|row| row.node.id == chapter).expect("大纲里应当有这一章");
    assert!(outline_row.lines.is_empty(), "没标故事线就是空的");

    // 另外几块没造数据：能读（授予了），只是空
    let fragments = reader.fragments(work).unwrap();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].id, fragment);
    assert_eq!(fragments[0].kind, FragmentKind::Idea);
    assert!(reader.entities(work).unwrap().is_empty());
    assert!(reader.scenes(work).unwrap().is_empty());
    assert!(reader.foreshadows(work).unwrap().is_empty());
}

#[test]
fn unknown_modules_cannot_get_a_reader() {
    let (_dir, store) = fresh();
    assert_eq!(
        store.module_reader("app.example.none").unwrap_err().code(),
        codes::MODULE_NOT_FOUND
    );
}

/// 读账：**读到的和被拒的都要留痕**（被拒也是事实，面板要能说"它试过"）。
#[test]
fn the_read_log_records_both_what_was_read_and_what_was_refused() {
    use yanmo_core::store::{recent_entries, ModuleIpcHandler, ReadLog, READ_LOG_FILE};
    use yanmo_proto::ipc::ReadHandler;

    let (dir, mut store) = fresh();
    let (work, chapter) = seeded(&mut store);
    store.install_module(&manifest(), &grant(&[CoreDataArea::Body])).unwrap();

    let log = ReadLog::new(dir.path().join(READ_LOG_FILE));
    {
        let second = Store::open(dir.path().join("yanmo.db")).unwrap();
        let mut handler = ModuleIpcHandler::new(second, ID).with_read_log(log.clone());
        let body = handler.read(CoreDataArea::Body, Some(&chapter.to_string())).unwrap();
        assert!(body.contains("第一章的正文"));
        // 大纲没授予：门卫拒了它，这一笔也要进账
        let refused = handler.read(CoreDataArea::Outline, Some(&work.to_string())).unwrap_err();
        assert_eq!(refused.code, codes::MODULE_CAPABILITY_DENIED);
    }

    let entries = recent_entries(log.path(), 10);
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_eq!(entries[0].area, "body");
    assert_eq!(entries[0].work, Some(work), "正文的节点 id 要换算成书");
    assert_eq!(entries[0].chars, "第一章的正文。".chars().count() as u64);
    assert!(entries[0].allowed());
    assert_eq!(entries[1].area, "outline");
    assert_eq!(entries[1].outcome, "denied");
    assert_eq!(entries[1].work, Some(work));
    assert_eq!(entries[1].code.as_deref(), Some(codes::MODULE_CAPABILITY_DENIED));
}

/// 恐慌按钮：**只停第三方**——官方模块（`app.yanmo.` 前缀）与稿子照常。
#[test]
fn the_panic_button_freezes_third_party_modules_only() {
    let (_dir, mut store) = fresh();
    let mut official = manifest();
    official.id = "app.yanmo.official".to_string();
    store.install_module(&manifest(), &grant(&[])).unwrap();
    store.install_module(&official, &grant(&[])).unwrap();

    assert_eq!(store.panic_modules().unwrap(), 1, "只停第三方那一个");
    assert!(!store.module_card(ID).unwrap().enabled);
    assert!(store.module_card("app.yanmo.official").unwrap().enabled, "官方模块不动");
    assert_eq!(store.panic_modules().unwrap(), 0, "再按一次没有可停的了");
}
