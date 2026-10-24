//! 护眼滤色（整窗一层暖黄膜，幅度可调）的验收。
//!
//! 四条判据：
//! 1. 新库出生就是**关**（0）——不给没提过这件事的人换底子；读默认不该顺手写库。
//! 2. `0` 是**正经取值**（＝完全不染），不是"清掉这一层"：写 0 要原样存下来；
//!    负数才是清掉（回默认），越界夹住、不报错（纯观感，不值得为它挡人）。
//! 3. **只走全局**：往某本书写它不该翻动界面，也不该在每书记录里留一个没人读的死键。
//! 4. ★ 它与正文排版、稿纸横线、正文背景同一条底线：**正文与导出一个字节都不许变**。

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{Appearance, ExportFormat, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

/// 库里某个键的原文（没这个键就是 None）。
fn raw(store: &Store, key: &str) -> Option<String> {
    store
        .conn()
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

#[test]
fn a_fresh_library_is_unfiltered() {
    let (_dir, store) = fresh();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0, "默认关：一个像素都不染");
    assert!(raw(&store, "appearance").is_none(), "读默认不该顺手写库");
}

#[test]
fn the_strength_is_remembered_and_zero_means_off() {
    let (_dir, mut store) = fresh();

    // 拉开滑杆：记住这个数
    store
        .set_appearance(None, &Appearance { eye_care: Some(45), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 45);
    assert!(
        raw(&store, "appearance").is_some_and(|json| json.contains("eye_care")),
        "只改这一项也要落库（漏了它，作者调完重开就没了）"
    );

    // ★ 0 是正经取值（＝关）**且要原样存下来**——不能拿它当"清掉这一层"
    //   （那样一来"先关一下看看"就变成一个改不掉的空档）
    store
        .set_appearance(None, &Appearance { eye_care: Some(0), ..Default::default() })
        .unwrap();
    let off = store.appearance(None).unwrap();
    assert_eq!(off.eye_care, 0);
    assert!(
        raw(&store, "appearance").is_some_and(|json| json.contains("eye_care")),
        "写 0 是「关」，不是「这项不要设了」：记录还得在"
    );

    // 负数 = 清掉这一层（回默认关）——与图片不透明度同一个口径
    store
        .set_appearance(None, &Appearance { eye_care: Some(-1), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0);
    assert!(raw(&store, "appearance").is_none(), "一项都不剩就把键清掉");

    // 越界夹住、**不报错**（纯观感：手滑写个离谱的数不该把设置面板挡在外面）
    store
        .set_appearance(None, &Appearance { eye_care: Some(180), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 100, "夹到上限");
    store
        .set_appearance(None, &Appearance { eye_care: Some(-99), ..Default::default() })
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0, "负数＝清掉，回默认关");
}

#[test]
fn the_filter_is_global_only() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    store
        .set_appearance(None, &Appearance { eye_care: Some(60), ..Default::default() })
        .unwrap();

    // ★ 只走全局：往这本书写它**不该**改变这本书生效的那一份
    store
        .set_appearance(Some(work.id), &Appearance { eye_care: Some(10), ..Default::default() })
        .unwrap();
    assert_eq!(
        store.appearance(Some(work.id)).unwrap().eye_care,
        60,
        "护眼滤色不参与每书覆盖：这本书跟全局"
    );
    let book_raw = raw(&store, &format!("work.{}.appearance", work.id));
    assert!(
        book_raw.as_deref().is_none_or(|json| !json.contains("eye_care")),
        "每书的记录里不该落一个永远不被读的死键：{book_raw:?}"
    );

    // 库里已有的坏值当没设过（跟坏口径 / 坏配色一条规矩）：读侧不报错，回默认关
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"eye_care\":-5}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0, "坏值回默认关");
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"eye_care\":200}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0, "越界的旧值也当没设过");
    store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at)
             VALUES('appearance', '{\"eye_care\":\"很黄\"}', 0)",
            [],
        )
        .unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0, "类型不对也当没设过");
}

/// ★ 这一条是这项功能的验收底线：它只改屏幕观感——**正文与导出一个字节都不许变**。
#[test]
fn the_filter_never_touches_the_text_or_the_export() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(volume), NodeKind::Chapter, "").unwrap();
    store.write_body(chapter, "第一段。\n第二段。").unwrap();

    let body_before = store.read_body(chapter).unwrap();
    let export_before: Vec<u8> = store
        .render_work(work.id, ExportFormat::Text)
        .unwrap()
        .iter()
        .flat_map(|file| file.content.clone())
        .collect();

    store
        .set_appearance(None, &Appearance { eye_care: Some(100), ..Default::default() })
        .unwrap();

    assert_eq!(store.read_body(chapter).unwrap(), body_before, "滤色不许碰正文");
    let export_after: Vec<u8> = store
        .render_work(work.id, ExportFormat::Text)
        .unwrap()
        .iter()
        .flat_map(|file| file.content.clone())
        .collect();
    assert_eq!(export_after, export_before, "滤色不许进导出");

    // 回到默认（关）之后同样一个字节都不变
    store.reset_appearance(None).unwrap();
    assert_eq!(store.appearance(None).unwrap().eye_care, 0);
    assert_eq!(store.read_body(chapter).unwrap(), body_before);
}
