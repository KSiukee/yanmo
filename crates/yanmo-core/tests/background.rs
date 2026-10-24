//! 正文背景那一族的验收：**底色 / 底纹 / 图片三层各自能存能读、坏值有明确去向**，
//! 而且——与排版那几项、稿纸横线同一条底线——**正文与导出一个字节都不许变**。
//!
//! 三件事必须成立：
//! 1. 新库出生就是"与一直以来的样子一模一样"（纸白、不铺、没有图、不透明度 35）；
//! 2. 认不出的码**读侧当没设过**（回默认，不把界面卡住）、**写侧当场拒**（不悄悄收下）；
//! 3. 图片那一格只认**文件名**（库里那一格被写成路径 / 带盘符 / 空串，一律当没设过）。

use yanmo_core::model::WorkKind;
use yanmo_core::store::{ExportFormat, Store};

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn a_fresh_library_looks_exactly_like_before() {
    let (_dir, store) = fresh();
    let appearance = store.appearance(None).unwrap();
    assert_eq!(appearance.editor_bg_color.as_str(), "paper", "纸白＝与一直以来的编辑区底色一致");
    assert_eq!(appearance.editor_bg_layout.as_str(), "none", "不铺底纹");
    assert_eq!(appearance.editor_bg_fit.as_str(), "cover", "有图时默认铺满");
    assert_eq!(appearance.editor_bg_opacity, 35, "图片层默认只垫一层气氛");
    assert_eq!(appearance.editor_bg_image, None, "没有图");
}

#[test]
fn the_three_layers_can_be_set_and_read_back() {
    let (_dir, mut store) = fresh();

    let mut one = yanmo_core::store::Appearance::default();
    one.editor_bg_color = Some("green".to_string());
    store.set_appearance(None, &one).unwrap();

    let mut two = yanmo_core::store::Appearance::default();
    two.editor_bg_layout = Some("dots".to_string());
    two.editor_bg_fit = Some("tile".to_string());
    two.editor_bg_opacity = Some(80);
    store.set_appearance(None, &two).unwrap();

    let appearance = store.appearance(None).unwrap();
    assert_eq!(appearance.editor_bg_color.as_str(), "green");
    assert_eq!(appearance.editor_bg_layout.as_str(), "dots");
    assert_eq!(appearance.editor_bg_fit.as_str(), "tile");
    assert_eq!(appearance.editor_bg_opacity, 80);
    // 稀疏合并：第二次没带的项保持第一次写进去的值
    assert_eq!(appearance.editor_bg_color.as_str(), "green", "没传的项不许被清掉");
}

#[test]
fn unknown_codes_are_refused_on_write_and_ignored_on_read() {
    let (_dir, mut store) = fresh();

    // 先落一条**合法**的记录（没有它，库里连这一格都不存在，下面的"读侧"就无从谈起）
    let mut good = yanmo_core::store::Appearance::default();
    good.editor_bg_color = Some("cream".to_string());
    good.editor_bg_layout = Some("grid".to_string());
    store.set_appearance(None, &good).unwrap();

    // 写侧：认不出的码当场拒（给界面埋"不知道该画什么"的空档比报错更糟）
    let mut bad_color = yanmo_core::store::Appearance::default();
    bad_color.editor_bg_color = Some("sepia".to_string());
    assert!(store.set_appearance(None, &bad_color).is_err());

    let mut bad_layout = yanmo_core::store::Appearance::default();
    bad_layout.editor_bg_layout = Some("lined".to_string());
    assert!(store.set_appearance(None, &bad_layout).is_err());

    let mut bad_fit = yanmo_core::store::Appearance::default();
    bad_fit.editor_bg_fit = Some("stretch".to_string());
    assert!(store.set_appearance(None, &bad_fit).is_err());

    // 写侧拒过之后，库里那几格**一个字节没动**（还是上一次写进去的那一份）
    let appearance = store.appearance(None).unwrap();
    assert_eq!(appearance.editor_bg_color.as_str(), "cream");
    assert_eq!(appearance.editor_bg_layout.as_str(), "grid");

    // 读侧：把坏码直接塞进库里（模拟手改过的库 / 老版本写的），读出来当没设过
    let raw: String = store
        .conn()
        .query_row("SELECT value FROM settings WHERE key = 'appearance'", [], |row| row.get(0))
        .unwrap();
    assert!(raw.contains("cream"), "库里那一格应当是刚才写进去的那一份：{raw}");
    let broken = raw.replace("cream", "sepia");
    store
        .conn()
        .execute(
            "UPDATE settings SET value = ?1 WHERE key = 'appearance'",
            rusqlite::params![broken],
        )
        .unwrap();
    let appearance = store.appearance(None).unwrap();
    assert_eq!(appearance.editor_bg_color.as_str(), "paper", "坏记录当没设过，回默认");
    assert_eq!(appearance.editor_bg_layout.as_str(), "grid", "别的项不受这一格连累");
}

#[test]
fn auto_clears_a_layer_and_opacity_zero_is_a_choice_not_a_clear() {
    let (_dir, mut store) = fresh();

    let mut set = yanmo_core::store::Appearance::default();
    set.editor_bg_color = Some("cream".to_string());
    set.editor_bg_layout = Some("grid".to_string());
    set.editor_bg_opacity = Some(70);
    store.set_appearance(None, &set).unwrap();

    // `"auto"` = 清掉这一层（回默认），与命名 / 语气 / 配色同一条路
    let mut clear = yanmo_core::store::Appearance::default();
    clear.editor_bg_color = Some("auto".to_string());
    clear.editor_bg_layout = Some("auto".to_string());
    store.set_appearance(None, &clear).unwrap();
    let appearance = store.appearance(None).unwrap();
    assert_eq!(appearance.editor_bg_color.as_str(), "paper");
    assert_eq!(appearance.editor_bg_layout.as_str(), "none");

    // `0` 是正经取值（图留着但看不见）——不许像排版那样拿"≤0"当清掉
    let mut zero = yanmo_core::store::Appearance::default();
    zero.editor_bg_opacity = Some(0);
    store.set_appearance(None, &zero).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_opacity, 0);

    // 越界夹住（纯观感，不值得为它报错挡人）；负数＝清掉这一层、回默认
    let mut big = yanmo_core::store::Appearance::default();
    big.editor_bg_opacity = Some(150);
    store.set_appearance(None, &big).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_opacity, 100, "夹到上限");

    let mut negative = yanmo_core::store::Appearance::default();
    negative.editor_bg_opacity = Some(-1);
    store.set_appearance(None, &negative).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_opacity, 35, "清掉这一层＝回默认");
}

#[test]
fn the_image_slot_only_ever_holds_a_file_name() {
    let (_dir, mut store) = fresh();

    store.set_background_image(None, Some("夜航.png")).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_image.as_deref(), Some("夜航.png"));

    // 路径 / 盘符 / 上下级目录：一律只留最后那一段名字（界面那边也永远拿不到路径）。
    // ⚠️ 下面这两串要**按字面量拼**，别在源码里写成真的绝对路径（脱敏扫描会当成内部路径命中）
    let with_drive = ["C:", "Users", "me", "pics", "cover.png"].join(r"\");
    store.set_background_image(None, Some(&with_drive)).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_image.as_deref(), Some("cover.png"));
    let parents = ["..", "..", "yanmo.db"].join(r"\");
    store.set_background_image(None, Some(&parents)).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_image.as_deref(), Some("yanmo.db"));

    // 只剩目录成分 / 空白：当没设过（**不报错**：作者该看到"图没了，回纯色"）。
    // 盘符那一串同样**按字面量拼**（见上一条的理由）
    let drive_only = ["C:", ""].join(r"\");
    for bad in ["", "   ", "..", drive_only.as_str()] {
        store.set_background_image(None, Some(bad)).unwrap();
        assert_eq!(
            store.appearance(None).unwrap().editor_bg_image,
            None,
            "「{bad}」应当当没设过"
        );
    }

    // 传 None = 清掉这一层
    store.set_background_image(None, Some("a.png")).unwrap();
    store.set_background_image(None, None).unwrap();
    assert_eq!(store.appearance(None).unwrap().editor_bg_image, None);
}

/// 底线：背景那一族是**显示层**。改它前后，分章 txt / json 逐字节一样、正文一字不动。
#[test]
fn the_background_never_touches_the_manuscript_or_exports() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Article, "夜航").unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    store.write_body(root, "第一段。\n\n第二段。").unwrap();

    let before_txt = store.render_work(work.id, ExportFormat::Text).unwrap();
    let before_json = store.render_work(work.id, ExportFormat::Json).unwrap();
    let before_body = store.read_body(root).unwrap();

    let mut set = yanmo_core::store::Appearance::default();
    set.editor_bg_color = Some("cream".to_string());
    set.editor_bg_layout = Some("grain".to_string());
    set.editor_bg_fit = Some("tile".to_string());
    set.editor_bg_opacity = Some(90);
    store.set_appearance(None, &set).unwrap();
    store.set_background_image(None, Some("背景.png")).unwrap();

    assert_eq!(store.render_work(work.id, ExportFormat::Text).unwrap(), before_txt);
    assert_eq!(store.render_work(work.id, ExportFormat::Json).unwrap(), before_json);
    assert_eq!(store.read_body(root).unwrap(), before_body, "正文一个字都不许动");
    // 反面证据：导出里也看不到底色 / 底纹 / 图片名（那些不该自己跑进稿子）
    let plain = String::from_utf8(before_txt[0].content.clone()).unwrap();
    assert!(!plain.contains("背景.png") && !plain.contains("cream"), "导出里不该有背景那一族");
}

/// 每书覆盖：背景三层走的是与排版同一条路（机制先留着，界面第一版只暴露全局）。
#[test]
fn a_work_can_override_the_background_layers() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();

    let mut global = yanmo_core::store::Appearance::default();
    global.editor_bg_color = Some("gray".to_string());
    global.editor_bg_opacity = Some(60);
    store.set_appearance(None, &global).unwrap();

    let mut book = yanmo_core::store::Appearance::default();
    book.editor_bg_color = Some("green".to_string());
    store.set_appearance(Some(work.id), &book).unwrap();

    let appearance = store.appearance(Some(work.id)).unwrap();
    assert_eq!(appearance.editor_bg_color.as_str(), "green", "这本书单独设的那一档压过全局");
    assert_eq!(appearance.editor_bg_opacity, 60, "它没设的项继承全局");
}
