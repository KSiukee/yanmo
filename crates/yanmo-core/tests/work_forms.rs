//! 作品形态定调的**验收核对**：三档形态跑得通，且形态不写死在表结构里。
//!
//! 定调（用户确认）：
//! - 研墨是**作品容器**（书架一等公民），不是"一本书"；
//! - 「长篇 / 单篇 / 短篇集」只是同一个容器的三种 `kind`，不是三种实体；
//! - 结构是**可变深度节点树**：「卷 / 章 / 节 / 篇 / 场景卡」只是 `node_kind` 的取值，
//!   **不是表结构**，也不许为某一种形态加特例字段；
//! - 三档形态共用这一棵树：① 零层级单篇；② 独立长文＝单篇 + 节；③ 长篇＝卷 → 章。
//!
//! 这个文件把"三档"各走一遍（新建 → 写正文 → **关库重开** → 导出与编译都带上了那段字），
//! 并把边界钉成机械断言。最后一条是**这次核对查出来的缺口**：见该测试的说明。

use std::collections::HashSet;

use yanmo_core::compile::{compile, CompileOptions, Preset};
use yanmo_core::model::{NodeKind, NamingStyle, WorkKind};
use yanmo_core::store::{Appearance, EditorCursor, ExportFormat, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

fn text_of(store: &Store, work_id: i64) -> String {
    store
        .render_work(work_id, ExportFormat::Text)
        .unwrap()
        .iter()
        .map(|file| String::from_utf8(file.content.clone()).unwrap())
        .collect()
}

fn merged_txt(store: &Store, work_id: i64) -> String {
    compile(store, work_id, Preset::MergedTxt, &CompileOptions::default())
        .unwrap()
        .iter()
        .map(|file| String::from_utf8(file.content.clone()).unwrap())
        .collect()
}

/// 三档形态：都用同一套 API（`create_work` / `create_node` / `write_body` / `render_work`）。
#[test]
fn all_three_forms_run_through_new_write_reopen_and_export() {
    let (dir, mut store) = fresh();

    // ① 零层级单篇：作品本身就是那一篇（建书时给的根节点就是它）
    let article = store.create_work(WorkKind::Article, "短记").unwrap();
    let piece = store.list_nodes(article.id).unwrap()[0].id;
    store.write_body(piece, "零层级：正文就在作品根上。").unwrap();

    // ② 独立长文：单篇 + 一层节（**数据层允许**；界面上还建不出来，见文件最后一条）
    let essay = store.create_work(WorkKind::Article, "长文").unwrap();
    let essay_root = store.list_nodes(essay.id).unwrap()[0].id;
    store.write_body(essay_root, "开头一段。").unwrap();
    for body in ["第一节的正文。", "第二节的正文。"] {
        let section = store.create_node(essay.id, Some(essay_root), NodeKind::Section, "").unwrap();
        store.write_body(section, body).unwrap();
    }

    // ③ 长篇：卷 → 章 → 节（三层，章的下面还能有节）
    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(novel.id).unwrap()[0].id;
    let chapter = store.create_node(novel.id, Some(volume), NodeKind::Chapter, "第一章").unwrap();
    store.write_body(chapter, "章的正文。").unwrap();
    // 章下面挂一节：数据层同样允许（章本就是既能装正文又能装下级的容器）
    let inner = store.create_node(novel.id, Some(chapter), NodeKind::Section, "第一节").unwrap();
    store.write_body(inner, "节里的字。").unwrap();

    // 关库重开：存住的东西才算数
    drop(store);
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();

    assert_eq!(store.read_body(piece).unwrap().trim(), "零层级：正文就在作品根上。");
    assert_eq!(store.get_work(essay.id).unwrap().title, "长文");
    assert_eq!(store.read_body(inner).unwrap().trim(), "节里的字。");
    assert_eq!(
        store.get_work(novel.id).unwrap().kind,
        WorkKind::Novel,
        "作品类型是库里的一等公民，重开之后还在"
    );

    // 导出：分章 txt 与合并 txt 都必须带上每一档的每一个字
    for (work, words) in [
        (article.id, vec!["零层级：正文就在作品根上。"]),
        (essay.id, vec!["开头一段。", "第一节的正文。", "第二节的正文。"]),
        (novel.id, vec!["章的正文。", "节里的字。"]),
    ] {
        let exported = text_of(&store, work);
        let merged = merged_txt(&store, work);
        for word in words {
            assert!(exported.contains(word), "分章导出少了《{}》里的：{word}", store.get_work(work).unwrap().title);
            assert!(merged.contains(word), "合并导出少了《{}》里的：{word}", store.get_work(work).unwrap().title);
        }
    }
}

/// 形态**不在表结构里**：`nodes` 表没有一列叫"卷"或"章"，也不缺"为长文特制的字段"。
///
/// 这一条是定调里"绝不允许把卷/章写死"的机械核对：一旦有人往表里加
/// `volume_id` / `chapter_no` 这类列，这里就会说话。
#[test]
fn the_shape_lives_in_data_not_in_the_schema() {
    let (_dir, store) = fresh();
    let mut stmt = store.conn().prepare("PRAGMA table_info(nodes)").unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(|row| row.ok())
        .collect();
    assert!(columns.contains(&"node_kind".to_string()), "形态是**一列取值**，不是一张表：{columns:?}");
    for shape in ["volume", "chapter", "section", "piece", "scene", "form", "shape"] {
        assert!(
            !columns.iter().any(|column| column == shape),
            "表结构里不该有专为某种形态准备的列：{shape}（实际列：{columns:?}）"
        );
    }

    // 反过来：任意类型都能挂在任意父级下——深度与搭配都不写死，由数据说了算
    let (_dir2, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "怪树").unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    let kinds = [
        NodeKind::Volume,
        NodeKind::Chapter,
        NodeKind::Section,
        NodeKind::Piece,
        NodeKind::Scene,
    ];
    let mut parents = vec![root];
    for kind in kinds {
        let child = store.create_node(work.id, Some(root), kind, "").unwrap();
        parents.push(child);
    }
    for parent in &parents {
        for kind in kinds {
            store
                .create_node(work.id, Some(*parent), kind, "")
                .unwrap_or_else(|error| panic!("{kind:?} 该能挂在 {parent} 下面：{error}"));
        }
    }
    let total = store.list_nodes(work.id).unwrap().len();
    assert_eq!(total, 1 + kinds.len() + parents.len() * kinds.len(), "每个组合都建出来了");
}

/// 作品不是全局单例：两个不同形态的作品可以同时开着，各记各的光标、各用各的命名档。
#[test]
fn works_of_different_forms_keep_their_own_state() {
    let (_dir, mut store) = fresh();
    let novel = store.create_work(WorkKind::Novel, "长篇").unwrap();
    let article = store.create_work(WorkKind::Article, "单篇").unwrap();
    store
        .set_appearance(
            Some(novel.id),
            &Appearance { naming: Some(NamingStyle::Chinese.as_str().to_string()), ..Default::default() },
        )
        .unwrap();

    let novel_chapter =
        store.create_node(novel.id, None, NodeKind::Chapter, "").unwrap_or_else(|_| {
            let volume = store.list_nodes(novel.id).unwrap()[0].id;
            store.create_node(novel.id, Some(volume), NodeKind::Chapter, "").unwrap()
        });
    let piece = store.list_nodes(article.id).unwrap()[0].id;
    store
        .save_cursor(novel_chapter, EditorCursor { anchor: 11, head: 11, scroll_top: 20 })
        .unwrap();
    store
        .save_cursor(piece, EditorCursor { anchor: 3, head: 3, scroll_top: 0 })
        .unwrap();

    // 各记各的光标（互不覆盖）
    assert_eq!(store.load_cursor(novel_chapter).unwrap().map(|c| c.anchor), Some(11));
    assert_eq!(store.load_cursor(piece).unwrap().map(|c| c.anchor), Some(3));
    // 各用各的命名档：长篇是中文数字，单篇（文集/单篇默认不编号）不受影响
    assert_eq!(store.naming_style(novel.id).unwrap(), NamingStyle::Chinese);
    assert_eq!(store.naming_style(article.id).unwrap(), NamingStyle::NoNumber);
    // 书架一视同仁列出两本（不分组、不给单篇单独入口）
    let shelf = store.shelf().unwrap();
    assert_eq!(shelf.len(), 2);
    let kinds: HashSet<&str> = shelf.iter().map(|entry| entry.work.kind.as_str()).collect();
    assert_eq!(kinds, HashSet::from(["novel", "article"]));
}

/// ★ 真机回归（这次核对查出来的）：**单篇下面挂了节，分章导出曾经把节里的字整章丢掉**。
///
/// 原因是导出按"这种类型能不能放下级"这道**界面可放性**判断要不要往下走，
/// 而单篇在能力表里是叶子 → 明明数据里有下级，它也不往下走。
/// 导出是"把作者的字带走"，只能按**数据**判有没有下级。
#[test]
fn a_piece_with_sections_exports_every_word() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "长文").unwrap();
    let piece = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(piece, "开头一段。").unwrap();
    let section = store.create_node(work.id, Some(piece), NodeKind::Section, "第一节").unwrap();
    store.write_body(section, "绝不能丢的字。").unwrap();

    let files = store.render_work(work.id, ExportFormat::Text).unwrap();
    let all: String = files
        .iter()
        .map(|file| String::from_utf8(file.content.clone()).unwrap())
        .collect();
    assert!(all.contains("绝不能丢的字。"), "节里的字必须出现在导出里：{files:?}");
    assert!(
        files.iter().any(|file| file.relative_path.contains('/')),
        "节应当收在单篇自己的目录里：{:?}",
        files.iter().map(|file| file.relative_path.clone()).collect::<Vec<_>>()
    );
    assert!(merged_txt(&store, work.id).contains("绝不能丢的字。"), "合并 txt 同样不能丢");
}

/// ★ 核对结论（缺口，**故意钉在这里**）：定调第 ② 档「独立长文＝单篇 + 一层节」，
/// 数据层已经支持（上一条测试证明），但**界面还建不出来**——
/// `NodeKind::Piece::accepts_children()` 是 `false`，而界面正是按它决定"能不能往里放"
/// （见 `DirectoryPane.vue` 的落点判定与 `editor/tree.ts` 的 `addIntent`）。
///
/// 为什么把它写成断言而不是只写一条备注：将来真去接通那一档时，这条会失败，
/// 逼着我们回来把"单篇能不能装节"这件事**显式**改对（而不是某天悄悄变了没人知道）。
#[test]
fn the_long_article_form_is_not_reachable_from_the_interface_yet() {
    assert!(
        !NodeKind::Piece.accepts_children(),
        "界面按这个能力位决定落点；改成 true 就说明「单篇 + 节」接通了，请连同这条测试一起更新"
    );
    // 数据层与界面层的落差正是缺口本身：能存进去，但界面上放不进去
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "长文").unwrap();
    let piece = store.list_nodes(work.id).unwrap()[0].id;
    assert!(
        store.create_node(work.id, Some(piece), NodeKind::Section, "第一节").is_ok(),
        "数据层允许（可变深度树），所以缺口只在界面与能力位"
    );
}
