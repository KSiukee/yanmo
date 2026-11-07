//! 窗口落点验收：**记住上次的位置与大小，但绝不让窗口跑到看不见的地方**。
//!
//! 判据：
//! 1. 没记过 → 没有落点（调用方回落到居中），且读一眼不写库；
//! 2. 记一次再读 → 原样回来；坏 JSON / 零尺寸 / 天文数字一律当没记过；
//! 3. 记录还够得着某块屏 → 原样（或夹进可用区）；
//! 4. 那块屏没了（副屏被拔、分辨率变小）→ 夹进剩下的屏；一块都够不着 → 没有落点；
//! 5. 「够得着」有下限：只露出 50 像素的边等于看不见，不能算。

use yanmo_core::store::{Store, WindowPlace, WorkArea};

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

/// 一块 1920×1080 的主屏（任务栏占掉底下 40 像素）。
fn main_screen() -> WorkArea {
    WorkArea { x: 0, y: 0, width: 1920, height: 1040 }
}

#[test]
fn nothing_is_remembered_until_the_first_close() {
    let (_dir, store) = fresh();
    assert_eq!(store.window_place().unwrap(), None);
    assert_eq!(raw(&store, "window.place"), None, "读一眼不该顺手写库");
}

#[test]
fn a_remembered_place_comes_back_unchanged() {
    let (_dir, store) = fresh();
    let place = WindowPlace { x: 240, y: 120, width: 1280, height: 820 };
    store.remember_window_place(place).unwrap();

    assert_eq!(store.window_place().unwrap(), Some(place));
    assert!(raw(&store, "window.place").unwrap().contains("1280"));

    // 再记一次：覆盖，不留两份
    let moved = WindowPlace { x: 10, y: 20, width: 1000, height: 700 };
    store.remember_window_place(moved).unwrap();
    assert_eq!(store.window_place().unwrap(), Some(moved));
}

#[test]
fn a_broken_record_is_treated_as_no_record_at_all() {
    let (_dir, store) = fresh();
    for junk in ["这不是 JSON", "{}", r#"{"x":0,"y":0}"#, "null"] {
        store
            .conn()
            .execute(
                "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES('window.place', ?1, 0)",
                [junk],
            )
            .unwrap();
        assert_eq!(store.window_place().unwrap(), None, "坏记录要当没记过：{junk}");
    }
}

#[test]
fn sizes_that_cannot_be_a_window_never_land_in_the_library() {
    let (_dir, store) = fresh();
    for bad in [
        WindowPlace { x: 0, y: 0, width: 0, height: 800 },
        WindowPlace { x: 0, y: 0, width: 1200, height: 0 },
        WindowPlace { x: 0, y: 0, width: 100_000, height: 800 },
    ] {
        assert!(!bad.is_sane(), "这些尺寸都说不通：{bad:?}");
        store.remember_window_place(bad).unwrap();
        assert_eq!(raw(&store, "window.place"), None, "说不通的尺寸不该落库：{bad:?}");
    }
}

// ── 落点几何：记录是一份会过期的事实 ──────────────────────────────────────

#[test]
fn a_place_that_still_fits_its_screen_is_left_alone() {
    let place = WindowPlace { x: 300, y: 200, width: 1280, height: 820 };
    assert_eq!(place.fit(&[main_screen()]), Some(place));
}

#[test]
fn a_second_screen_that_is_gone_falls_back_instead_of_hiding() {
    // 上次摆在右边的副屏上（它的原点在 1920），副屏拔掉后只剩主屏
    let place = WindowPlace { x: 2300, y: 150, width: 1280, height: 820 };
    assert_eq!(place.fit(&[main_screen()]), None, "够不着就回落到居中，不是硬摆过去");
}

#[test]
fn a_screen_that_shrank_keeps_the_window_fully_inside() {
    // 记录来自 2560×1440，现在这块屏只有 1280×720（可用区 1280×680）
    let place = WindowPlace { x: 400, y: 260, width: 2000, height: 1100 };
    let small = WorkArea { x: 0, y: 0, width: 1280, height: 680 };
    assert_eq!(
        place.fit(&[small]),
        Some(WindowPlace { x: 0, y: 0, width: 1280, height: 680 }),
        "缩得下就缩，并夹回可用区——不许比屏幕还大"
    );
}

#[test]
fn a_screen_to_the_left_with_negative_coordinates_is_a_normal_screen() {
    let left = WorkArea { x: -1920, y: 0, width: 1920, height: 1040 };
    let place = WindowPlace { x: -1600, y: 100, width: 1280, height: 820 };
    assert_eq!(place.fit(&[main_screen(), left]), Some(place), "负坐标只是坐标，不是坏记录");
}

#[test]
fn a_place_poking_out_of_the_corner_is_pulled_back_in() {
    // 右下角探出去：仍然看得见（交集够大），但不能留着半截在外
    let place = WindowPlace { x: 1500, y: 900, width: 1280, height: 820 };
    assert_eq!(
        place.fit(&[main_screen()]),
        Some(WindowPlace { x: 640, y: 220, width: 1280, height: 820 }),
        "位置夹回可用区，尺寸不动"
    );
}

#[test]
fn a_sliver_of_a_window_does_not_count_as_visible() {
    // 只剩 50 像素的边露在屏幕里：看得见才怪
    let place = WindowPlace { x: 1870, y: 300, width: 1280, height: 820 };
    assert_eq!(place.fit(&[main_screen()]), None);

    // 刚好够 160×80 的那条边：算看得见，并且夹进屏内
    let edge = WindowPlace { x: 1760, y: 960, width: 1280, height: 820 };
    assert_eq!(
        edge.fit(&[main_screen()]),
        Some(WindowPlace { x: 640, y: 220, width: 1280, height: 820 })
    );
}

#[test]
fn with_several_reachable_screens_the_most_visible_one_wins() {
    let right = WorkArea { x: 1920, y: 0, width: 1920, height: 1040 };
    // 大部分在右边那块屏上，一小角搭在主屏上
    let place = WindowPlace { x: 1800, y: 300, width: 1280, height: 820 };
    assert_eq!(
        place.fit(&[main_screen(), right]),
        Some(WindowPlace { x: 1920, y: 220, width: 1280, height: 820 }),
        "按交集面积选屏，再夹进它的可用区"
    );
}

#[test]
fn no_screens_at_all_means_no_place() {
    let place = WindowPlace { x: 0, y: 0, width: 1280, height: 820 };
    assert_eq!(place.fit(&[]), None, "问不到屏幕时宁可回落到居中");
}
