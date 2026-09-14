//! 从成稿导入的验收：**库没了，字还在不在**。
//!
//! 这条路不碰 SQLite 快照，只读备份包里的 `成稿/<书名>/work.json`——所以验收的重点是
//! "读回来的东西跟原件是不是同一份"，而不是"函数跑完了没报错"：
//!
//! 1. 成稿读回来再导出，**逐字节一致**（四档编号规则各来一遍：号是算出来的，不能读回来变文字）；
//! 2. 空章 / 没起名的卷 / 深层嵌套 / 多本书都活着；
//! 3. 与备份清单对账：章节数、三口径字数、分章文本指纹 —— 对不上时**说得出来差在哪**；
//! 4. 坏 JSON 一律拒绝（宁可让作者拿原件来问，也不导进去半本）；
//! 5. 导进来的字**检索得到**（目录与正文都进得去），而既有作品一个字都不动。

use yanmo_core::model::{NodeKind, NamingStyle, WorkKind, WorkLanguage};
use yanmo_core::store::{
    find_drafts, parse_work_json, read_manifest, Appearance, BackupRequest, BackupTarget,
    ExportFormat, Store,
};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 一本"什么都有"的书：两卷（第一卷**没起名**）、四章（其中一章空）、一节套在章里、
/// 一章的场景卡，正文里有标点与换行。
fn rich_book(store: &mut Store, style: NamingStyle) -> i64 {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    store
        .set_appearance(
            Some(work.id),
            &Appearance { naming: Some(style.as_str().to_string()), ..Default::default() },
        )
        .unwrap();
    let first = store.list_nodes(work.id).unwrap()[0].id; // 建书时留白的那一卷：**没起名**
    let second = store.create_node(work.id, None, NodeKind::Volume, "夜行").unwrap();
    let one = store.create_node(work.id, Some(first), NodeKind::Chapter, "").unwrap();
    store.write_body(one, "雨下了整夜。\n\n他把铜钱按在桌上！").unwrap();
    let two = store.create_node(work.id, Some(first), NodeKind::Chapter, "自起的名字").unwrap();
    store.write_body(two, "序章这类不该占号。").unwrap();
    let section = store.create_node(work.id, Some(one), NodeKind::Section, "").unwrap();
    store.write_body(section, "节里的字。").unwrap();
    let empty = store.create_node(work.id, Some(second), NodeKind::Chapter, "").unwrap();
    store.write_body(empty, "").unwrap();
    store.create_node(work.id, Some(second), NodeKind::Scene, "").unwrap();
    work.id
}

fn json_of(store: &Store, work_id: i64) -> Vec<u8> {
    store.render_work(work_id, ExportFormat::Json).unwrap()[0].content.clone()
}

fn import(store: &mut Store, bytes: &[u8]) -> yanmo_core::store::ImportReport {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let draft = parse_work_json(&text).unwrap();
    let language = draft.language.unwrap_or(WorkLanguage::Zh);
    store.import_draft(&draft, language, "测试").unwrap()
}

#[test]
fn a_draft_reads_back_byte_for_byte_in_every_numbering_style() {
    for style in NamingStyle::ALL {
        let (_dir, mut source) = fresh();
        let work = rich_book(&mut source, style);
        let before = json_of(&source, work);

        // 空库 = "库没了，只剩成稿"：导入只认这份 JSON，不看任何快照
        let (_target_dir, mut target) = fresh();
        let report = import(&mut target, &before);
        let after = json_of(&target, report.work_id);

        assert_eq!(
            String::from_utf8(before).unwrap(),
            String::from_utf8(after).unwrap(),
            "{style:?} 这一档：成稿读回来再导出，必须一个字节都不差"
        );
        // 不编号那一档本来就没有号可算（原文与显示名一致才是对的）
        let expected = if style == NamingStyle::NoNumber { 0 } else { 1 };
        assert!(
            report.scale.templated >= expected,
            "{style:?} 这一档：号必须还是「算出来的」（原文 ≠ 显示名），不能读回来就成了钉死的文字"
        );
    }
}

#[test]
fn empty_chapters_unnamed_volumes_and_deep_nesting_all_survive() {
    let (_dir, mut source) = fresh();
    let work = source.create_work(WorkKind::Novel, "深井").unwrap();
    let root = source.list_nodes(work.id).unwrap()[0].id;
    // 三十二层：目录的深度不是写死的（`MAX_TREE_DEPTH` 之内都要能进能出）
    let mut parent = root;
    let mut deep = 0;
    for _ in 0..32 {
        parent = source.create_node(work.id, Some(parent), NodeKind::Section, "").unwrap();
        deep += 1;
    }
    source.write_body(parent, "井底的字。").unwrap();
    let empty = source.create_node(work.id, Some(root), NodeKind::Chapter, "").unwrap();
    source.write_body(empty, "").unwrap();
    let before = json_of(&source, work.id);

    let (_target_dir, mut target) = fresh();
    let report = import(&mut target, &before);
    assert_eq!(json_of(&target, report.work_id), before, "深层与空章一起都得原样回来");

    // 读回来之后：卷还是**没起名**（那条"占位名不落库"的规矩没被导入破坏）
    let nodes = target.list_nodes(report.work_id).unwrap();
    let volume = nodes.iter().find(|node| node.kind == NodeKind::Volume).unwrap();
    assert_eq!(volume.title, "", "没起名的卷读回来还是没起名");
    assert_eq!(volume.title_rendered, "第1卷", "显示名照旧按档算出来");
    assert_eq!(nodes.len(), 1 + deep + 1, "结构一个节点都不少");
    // 章数与字数：空章不算正文
    assert_eq!(report.scale.chapters, 1);
    assert_eq!(report.scale.bodies, 1, "只有井底那一个节点有正文");
}

#[test]
fn several_books_come_in_side_by_side_and_never_touch_what_was_there() {
    let (_dir, mut source) = fresh();
    let first = rich_book(&mut source, NamingStyle::Chinese);
    let second = source.create_work(WorkKind::Article, "短歌").unwrap();
    let piece = source.list_nodes(second.id).unwrap()[0].id;
    source.write_body(piece, "短歌的正文。").unwrap();
    let drafts = [json_of(&source, first), json_of(&source, second.id)];

    // 目标库里先有一本**自己要写的**书：导入不许碰它
    let (_target_dir, mut target) = fresh();
    let mine = target.create_work(WorkKind::Novel, "我自己的书").unwrap();
    let my_volume = target.list_nodes(mine.id).unwrap()[0].id;
    let my_chapter = target.create_node(mine.id, Some(my_volume), NodeKind::Chapter, "第一章").unwrap();
    target.write_body(my_chapter, "我自己写的字。").unwrap();
    let before = json_of(&target, mine.id);

    let one = import(&mut target, &drafts[0]);
    let two = import(&mut target, &drafts[1]);
    assert_ne!(one.work_id, two.work_id);
    assert_eq!(json_of(&target, one.work_id), drafts[0]);
    assert_eq!(json_of(&target, two.work_id), drafts[1]);
    assert_eq!(json_of(&target, mine.id), before, "既有作品一个字都不许动");
    assert_eq!(target.list_works().unwrap().len(), 3);
}

#[test]
fn reading_it_back_makes_the_text_searchable() {
    let (_dir, mut source) = fresh();
    let work = rich_book(&mut source, NamingStyle::Arabic);
    let before = json_of(&source, work);

    let (_target_dir, mut target) = fresh();
    let report = import(&mut target, &before);
    let by_body = target.search("铜钱按在桌上", None, 0).unwrap();
    assert!(
        by_body.iter().any(|hit| hit.work_id == report.work_id),
        "导进来的正文要进全文索引：{by_body:?}"
    );
    let by_title = target.search("自起的名字", None, 0).unwrap();
    assert!(
        by_title.iter().any(|hit| hit.work_id == report.work_id),
        "标题也要进索引：{by_title:?}"
    );
}

#[test]
fn the_import_itself_is_recorded_and_the_rows_are_not_counted_as_writing() {
    let (_dir, mut source) = fresh();
    let work = rich_book(&mut source, NamingStyle::Arabic);
    let before = json_of(&source, work);

    let (_target_dir, mut target) = fresh();
    let report = import(&mut target, &before);

    // 留痕：这一次导入记一笔（**不逐章伪造"每章各自诞生"**——那些行没有真实创作时间）
    let entries: Vec<(String, String)> = {
        let mut stmt = target
            .conn()
            .prepare("SELECT entity, op FROM op_log ORDER BY seq")
            .unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        rows.filter_map(|row| row.ok()).collect()
    };
    assert_eq!(entries, vec![("works".to_string(), "import".to_string())]);

    // 每日码字账本里一个字都不该记：导入的不是"作者今天敲出来的字"
    let days: i64 = target
        .conn()
        .query_row("SELECT COUNT(*) FROM writing_days", [], |r| r.get(0))
        .unwrap();
    assert_eq!(days, 0, "导入不能进「每日码字」账本");
    assert!(report.scale.word_count > 0, "但字数本身要算出来");
}

#[test]
fn the_backup_manifest_agrees_with_the_draft_it_shipped() {
    let (_dir, mut source) = fresh();
    rich_book(&mut source, NamingStyle::Padded);
    let target_dir = _dir.path().join("备份盘");
    let report = source
        .backup_now(&BackupRequest {
            data_dir: _dir.path().to_path_buf(),
            targets: vec![BackupTarget {
                path: target_dir.to_string_lossy().to_string(),
                volume_id: "vol-1".to_string(),
                volume_label: "备份盘".to_string(),
                removable: false,
            }],
            keep: 3,
            tz_offset_minutes: 480,
            device: "测试机".to_string(),
        })
        .unwrap();
    let package = std::path::PathBuf::from(&report.outcomes[0].package);
    let stamp = read_manifest(&package).unwrap().works[0].clone();

    // 把快照删掉：只剩成稿也要救得回来（这就是这条路存在的理由）
    std::fs::remove_file(package.join("yanmo.db")).unwrap();

    let drafts = find_drafts(&package);
    assert_eq!(drafts.len(), 1, "包里该找得到那一份成稿：{drafts:?}");
    let text = std::fs::read_to_string(&drafts[0]).unwrap();
    let draft = parse_work_json(&text).unwrap();
    assert!(
        draft.compare(&stamp).is_empty(),
        "成稿与清单对不上：{:?}",
        draft.compare(&stamp)
    );

    let (_target_dir, mut target) = fresh();
    let imported = import(&mut target, text.as_bytes());
    assert!(
        imported.scale.compare(&stamp, &imported.fingerprint).is_empty(),
        "写进库里之后也要对得上：{:?}",
        imported.scale.compare(&stamp, &imported.fingerprint)
    );
    assert_eq!(imported.scale.chapters, stamp.chapters);
    assert_eq!(imported.scale.word_count, stamp.word_count);

    // 反面对照：把成稿动一下，对账必须**说得出来差在哪**（不然这就是一道假绿）
    let mut broken: serde_json::Value = serde_json::from_str(&text).unwrap();
    broken["nodes"][0]["children"].as_array_mut().unwrap().remove(0);
    let broken_draft = parse_work_json(&serde_json::to_string(&broken).unwrap()).unwrap();
    let problems = broken_draft.compare(&stamp);
    assert!(
        problems.iter().any(|item| item.field == "chapters"),
        "少了一章必须报出来：{problems:?}"
    );
}

#[test]
fn a_bad_draft_is_refused_instead_of_half_imported() {
    // （「认不出的类型」那几个用的是核心既有的取值码——那几句字典里的话比"成稿不合格"更有用）
    let cases: [(&str, &str, &str); 10] = [
        ("空文件", "", "import.draft_invalid"),
        ("不是 JSON", "看起来像成稿", "import.draft_invalid"),
        ("不是对象", "[1, 2, 3]", "import.draft_invalid"),
        ("缺 title", r#"{"kind":"novel","nodes":[]}"#, "import.draft_invalid"),
        ("title 不是字符串", r#"{"title":7,"kind":"novel","nodes":[]}"#, "import.draft_invalid"),
        ("不认识的类型", r#"{"title":"x","kind":"史诗","nodes":[]}"#, "value.unknown_work_kind"),
        ("nodes 不是数组", r#"{"title":"x","kind":"novel","nodes":{}}"#, "import.draft_invalid"),
        (
            "多出不相干的格子",
            r#"{"title":"x","kind":"novel","nodes":[],"word":1}"#,
            "import.draft_invalid",
        ),
        (
            "卷上带了正文",
            r#"{"title":"x","kind":"novel","nodes":[{"kind":"volume","title":"第一卷","body":"字"}]}"#,
            "import.draft_invalid",
        ),
        (
            "认不出的节点类型",
            r#"{"title":"x","kind":"novel","nodes":[{"kind":"章","title":"第一章"}]}"#,
            "value.unknown_node_kind",
        ),
    ];
    for (what, text, expected) in cases {
        let error = parse_work_json(text).unwrap_err();
        assert_eq!(error.code(), expected, "{what} 该被当场拒绝：{error}");
    }

    // 拒绝归拒绝：**库里一个节点都不许留下**（半个作品比报错难查得多）
    let (_dir, mut store) = fresh();
    for (_, text, _) in cases {
        if let Ok(draft) = parse_work_json(text) {
            let _ = store.import_draft(&draft, WorkLanguage::Zh, "测试");
        }
    }
    assert_eq!(store.list_works().unwrap().len(), 0, "拒绝的稿子不该在书架上留下半本");
}

#[test]
fn drafts_are_found_in_both_the_old_and_the_new_package_layout() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // 老布局：整包只有一份 work.json
    std::fs::create_dir_all(root.join("成稿")).unwrap();
    std::fs::write(root.join("成稿/work.json"), "{}").unwrap();
    // 新布局：一本书一个目录
    std::fs::create_dir_all(root.join("成稿/长夜")).unwrap();
    std::fs::write(root.join("成稿/长夜/work.json"), "{}").unwrap();
    // 我们自己写在包里的清单不是成稿；别的 json 一律当候选（宁可当场报错，也不默默跳过）
    std::fs::write(root.join("成稿/manifest.json"), "{}").unwrap();
    std::fs::write(root.join("manifest.json"), "{}").unwrap();
    std::fs::write(root.join("成稿/别人的.json"), "{}").unwrap();

    let found = find_drafts(root);
    let names: Vec<String> = found
        .iter()
        .map(|path| path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    assert_eq!(
        names,
        vec!["成稿/work.json", "成稿/别人的.json", "成稿/长夜/work.json"],
        "两种布局都要认；清单不算成稿；顺序按路径排（每次跑都一样）"
    );

    // 作者把成稿文件夹本身指过来也认
    let inside = find_drafts(&root.join("成稿"));
    assert_eq!(inside.len(), 3, "把成稿文件夹本身指过来也认：{inside:?}");
}
