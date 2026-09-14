//! 跨版本升级不丢稿 + 只读不动库。
//!
//! 这两条守的是**同一件事的两个时刻**：
//! - ② 升级那一刻：程序化造一份"上一版 schema"的库（多本书 / 卷章树 / 正文 / 三档字数 /
//!   偏好 / 快照 / 回收站都有），用当前引擎打开迁移，**逐项断言一个都没少**；
//! - ③ 平时读的时候：打开 + 读一遍之后，库里**一个字节都没变**（抓"读路径顺手写了什么"
//!   这种最难发现的问题）。
//!
//! 为什么值得单独一个文件：它们是"发布之后某次升级把稿写炸"这类事故的唯一自动化防线，
//! 与平时那些功能测试不是一个目的——这里只关心**别丢东西**。

use std::collections::BTreeMap;

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{Appearance, ExportFormat, Store};

/// 一份"像真在用的库"：两本书（一本长篇带卷、一本零层级单篇），正文带标点与中文，
/// 还有偏好、快照、回收站里的东西。
struct Fixture {
    novel: i64,
    chapters: Vec<i64>,
    piece: i64,
    /// 单篇那本书的作品 id（正文那一篇的 id 是 `piece`，两个别混）
    piece_work: i64,
}

/// 正文：**逐字节**要比对，所以刻意放中文、标点、换行、英文与数字
const BODIES: &[&str] = &[
    "第一章的正文。\n\n他推开门，风雪灌了进来——屋里只有一盏灯。\n\n灯下有人。",
    "第二章：那人抬起头，说了一句谁也听不懂的话（听起来像某个地方的方言）。\n\n尾注：1. 未完。",
    "第三章很短。\n",
    "零层级的那一篇：没有卷，也没有编号。\n\n它也得活下来。",
];

fn build(path: &std::path::Path) -> Fixture {
    let mut store = Store::open(path).unwrap();

    let novel = store.create_work(WorkKind::Novel, "长夜").unwrap().id;
    let volume = store.create_node(novel, None, NodeKind::Volume, "第一卷").unwrap();
    let mut chapters = Vec::new();
    for (at, body) in BODIES.iter().take(3).enumerate() {
        let chapter = store
            .create_node(novel, Some(volume), NodeKind::Chapter, &format!("第{}章 灯", at + 1))
            .unwrap();
        // 用**带记账**的写法（编辑器落盘走的就是这条）：账本才有真实行可比
        store.write_body_counted(chapter, body, 480).unwrap();
        // 每章留一份快照：升级之后版本历史也得在
        store.snapshot_if_changed(chapter, "manual").unwrap();
        chapters.push(chapter);
    }

    // 零层级单篇：作品类型不同、没有卷，正文在多行之外还带尾部换行
    let piece_work = store.create_work(WorkKind::Article, "零篇").unwrap().id;
    let piece = store.create_node(piece_work, None, NodeKind::Piece, "零层级那一篇").unwrap();
    store.write_body_counted(piece, BODIES[3], 480).unwrap();

    // 偏好（全局 + 每书覆盖各一层）与回收站里的一项
    store
        .set_appearance(
            None,
            &Appearance {
                quote_style: Some("corner".into()),
                daily_goal: Some(1500),
                ..Default::default()
            },
        )
        .unwrap();
    store
        .set_appearance(
            Some(novel),
            &Appearance {
                daily_goal: Some(800),
                ..Default::default()
            },
        )
        .unwrap();
    let doomed = store
        .create_node(novel, Some(volume), NodeKind::Chapter, "会被删掉的一章")
        .unwrap();
    store.write_body_counted(doomed, "删之前的正文也留着。", 480).unwrap();
    store.soft_delete_node(doomed).unwrap();

    Fixture { novel, chapters, piece, piece_work }
}

/// 把"稿子还在不在"压成可以逐字比对的几组值。
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    /// 每处正文的**原样字节**（不是字符串比较：不放过任何一个字节的差别）
    bodies: BTreeMap<i64, Vec<u8>>,
    /// 目录树（含 parent / sort_order / 三档字数 / 渲染标题）——结构、顺序、计数一次比完
    trees: Vec<(i64, Vec<String>)>,
    /// 偏好（全局那份 + 长篇那份）
    global_prefs: String,
    novel_prefs: String,
    /// 回收站里的东西（标题与"带走了几项"）
    trash: Vec<String>,
    /// 快照：每章的份数与内容
    snapshots: BTreeMap<i64, Vec<String>>,
    /// 账本：三档口径
    ledger: Vec<(String, i64, i64, i64, i64)>,
}

fn take(store: &Store, fixture: &Fixture) -> Snapshot {
    let mut bodies = BTreeMap::new();
    let mut snapshots = BTreeMap::new();
    let mut ids = fixture.chapters.clone();
    ids.push(fixture.piece);
    for id in &ids {
        bodies.insert(*id, store.read_body(*id).unwrap().into_bytes());
        let mut rows = Vec::new();
        for summary in store.list_snapshots(*id).unwrap() {
            rows.push(format!("{}|{}", summary.id, store.snapshot_body(summary.id).unwrap()));
        }
        snapshots.insert(*id, rows);
    }

    let trees = [fixture.novel, fixture.piece_work]
        .iter()
        .map(|work| {
            let rows = store
                .list_nodes(*work)
                .unwrap()
                .iter()
                .map(|node| {
                    format!(
                        "{}|{:?}|{}|{:?}|{}|{}|{}|{}|{}",
                        node.id,
                        node.parent_id,
                        node.sort_order,
                        node.kind,
                        node.title,
                        node.title_rendered,
                        node.char_count,
                        node.chars_no_punct,
                        node.word_count
                    )
                })
                .collect::<Vec<_>>();
            (*work, rows)
        })
        .collect();

    let trash = store
        .list_trash()
        .unwrap()
        .iter()
        .map(|entry| format!("{}|{}|{}|{}", entry.id, entry.title, entry.work_title, entry.nodes))
        .collect();

    // 账本按天/书排序取全量（升级后一条都不该少）
    let mut ledger = Vec::new();
    let mut stmt = store
        .conn()
        .prepare("SELECT day, work_id, chars, chars_no_punct, words FROM writing_days ORDER BY day, work_id")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .unwrap();
    for row in rows {
        ledger.push(row.unwrap());
    }

    Snapshot {
        bodies,
        trees,
        global_prefs: format!("{:?}", store.appearance(None).unwrap()),
        novel_prefs: format!("{:?}", store.appearance(Some(fixture.novel)).unwrap()),
        trash,
        snapshots,
        ledger,
    }
}

#[test]
fn upgrade_from_previous_schema_keeps_every_manuscript_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");

    let fixture = build(&path);
    let before = {
        let store = Store::open(&path).unwrap();
        take(&store, &fixture)
    };
    assert!(!before.bodies.is_empty() && !before.trash.is_empty(), "夹具该有正文与回收站内容");
    assert!(!before.ledger.is_empty(), "夹具该有账本记录——空账本会让升级比对变成空转（绿得没意义）");
    assert!(!before.snapshots.values().all(Vec::is_empty), "夹具该有快照");

    // 把库退回"上一版 schema"：v7 只建了 writing_days（幂等），所以把表删掉 + 版本号退一格，
    // 就是一份诚实可信的 v6 库——不是伪造的，是**真的按上一版的形状**摆回去
    {
        let conn = yanmo_core::db::open(&path).unwrap();
        conn.execute("DROP TABLE writing_days", []).unwrap();
        conn.pragma_update(None, "user_version", 6).unwrap();
    }

    // 用当前引擎打开：结构升级
    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version(),
        "旧库应当被升到最新结构版本"
    );
    assert!(
        store
            .conn()
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='writing_days'",
                [],
                |r| r.get::<_, String>(0),
            )
            .is_ok(),
        "升级应当把 v7 的账本表建出来"
    );

    // 逐项比对：**升级之后不许有任何一处对不上**
    //
    // ⚠️ 账本（writing_days）不在这一组里比：**真实的 v6 库里根本没有这张表**
    //（它是 v7 才建的），没东西可"保"。它的忠诚度由下面那条"迁移重跑"的测试盯。
    let after = take(&store, &fixture);
    assert_eq!(after.bodies, before.bodies, "正文必须逐字节一致（这是本任务的核心断言）");
    assert_eq!(after.trees, before.trees, "树结构 / 阅读顺序 / 三档字数都不许变");
    assert_eq!(after.global_prefs, before.global_prefs, "全局偏好不许变");
    assert_eq!(after.novel_prefs, before.novel_prefs, "每书覆盖的偏好不许变");
    assert_eq!(after.trash, before.trash, "回收站里的东西不许丢");
    assert_eq!(after.snapshots, before.snapshots, "版本快照不许丢");
}

/// 迁移**重跑**也不许毁数据：把版本号退回上一格再打开，v7 会再走一遍
///（它建表建索引都带 `IF NOT EXISTS`，本来就该幂等）。
///
/// 为什么单独测这一条：真机上"升级到一半断电/被杀"就长这样——版本号没推进，下次启动重跑。
/// 如果重跑会清表或重建，作者的账本、正文就悄悄没了，而且**没人会发现**。
#[test]
fn rerunning_the_last_migration_keeps_the_ledger_and_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let fixture = build(&path);
    let before = {
        let store = Store::open(&path).unwrap();
        take(&store, &fixture)
    };
    assert!(!before.ledger.is_empty(), "夹具该有账本记录，否则这条测试是空转");

    {
        let conn = yanmo_core::db::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 6).unwrap(); // 只退版本号，表都留着
    }

    let store = Store::open(&path).unwrap();
    assert_eq!(
        yanmo_core::db::migrations::user_version(store.conn()).unwrap(),
        yanmo_core::db::migrations::schema_version()
    );
    let after = take(&store, &fixture);
    assert_eq!(after.ledger, before.ledger, "迁移重跑把账本弄丢了");
    assert_eq!(after.bodies, before.bodies, "迁移重跑把正文改了");
    assert_eq!(after.trees, before.trees, "迁移重跑把树改了");
    assert_eq!(after.snapshots, before.snapshots, "迁移重跑把快照弄丢了");
}

#[test]
fn reading_the_library_never_writes_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yanmo.db");
    let fixture = build(&path);

    // 先记下"库里现在是什么样"——**逻辑快照**而不是文件字节：
    // 库是 WAL 模式，关库时的 checkpoint 会动主库文件（那是存储引擎的正常收尾），
    // 按字节比会把正常收尾误报成"读写了库"。所以按**每张表每一行**比，这才是"内容有没有变"。
    let before = {
        let store = Store::open(&path).unwrap();
        logical_dump(&store)
    };

    {
        let store = Store::open(&path).unwrap();
        // 把"只读的一遍"跑全：目录树、逐章正文、检索、导出（三种格式）、体检
        let tree = store.list_nodes(fixture.novel).unwrap();
        assert!(!tree.is_empty());
        for id in fixture.chapters.iter().chain(std::iter::once(&fixture.piece)) {
            assert!(!store.read_body(*id).unwrap().is_empty());
            let _ = store.search("风雪", None, 10).unwrap();
            let _ = store.list_snapshots(*id).unwrap();
        }
        for format in [ExportFormat::Json, ExportFormat::Text] {
            let files = store.render_work(fixture.novel, format).unwrap();
            assert!(!files.is_empty(), "导出该有产物：{format:?}");
        }
        let _ = store.list_trash().unwrap();
        let _ = yanmo_core::db::quick_check(store.conn()).unwrap();
    }

    // 再打开、再快照：**一个字节都不许变**
    let after = {
        let store = Store::open(&path).unwrap();
        logical_dump(&store)
    };
    let changed: Vec<&String> = after
        .iter()
        .filter(|(table, rows)| before.get(*table) != Some(rows))
        .map(|(table, _)| table)
        .collect();
    let added: Vec<&String> = after.keys().filter(|table| !before.contains_key(*table)).collect();
    let removed: Vec<&String> = before.keys().filter(|table| !after.contains_key(*table)).collect();
    assert!(
        changed.is_empty() && added.is_empty() && removed.is_empty(),
        "只读的一遍动了库：内容变了 {changed:?}、多了表 {added:?}、少了表 {removed:?}"
    );
}

/// 每张表、每一行、按主键顺序摊成文本——"内容有没有变"的判据。
fn logical_dump(store: &Store) -> BTreeMap<String, Vec<String>> {
    let tables: Vec<String> = {
        let mut stmt = store
            .conn()
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|row| row.unwrap()).collect()
    };

    let mut dump = BTreeMap::new();
    for table in tables {
        if table.starts_with("node_fts") {
            continue; // 检索索引由触发器维护，单独看它会把"索引与正文的一致性"混进来
        }
        let mut rows = Vec::new();
        let mut stmt = store
            .conn()
            .prepare(&format!("SELECT * FROM \"{table}\""))
            .unwrap();
        let columns = stmt.column_count();
        let mapped = stmt
            .query_map([], |row| {
                let mut cells = Vec::new();
                for at in 0..columns {
                    cells.push(
                        row.get::<_, Option<String>>(at)
                            .map(|value| value.unwrap_or_default())
                            .or_else(|_| row.get::<_, Option<i64>>(at).map(|v| format!("{:?}", v)))
                            .unwrap_or_else(|_| "?".into()),
                    );
                }
                Ok(cells.join("|"))
            })
            .unwrap();
        for row in mapped {
            rows.push(row.unwrap());
        }
        rows.sort();
        dump.insert(table, rows);
    }
    dump
}
