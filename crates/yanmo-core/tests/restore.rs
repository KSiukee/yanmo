//! 从备份恢复的验收：**作者真能把稿子拿回来，而且敢按这个按钮**。
//!
//! 八条判据：
//! 1. 备份位置里的包列得出来（别人的目录、名字像但没有清单的，都不列）；
//! 2. 恢复前给得出体检结论与"会退回几天 / 少多少字"；
//! 3. 体检不过的（快照坏 / 成稿缺 / 来自更新的研墨）一律 `can_restore = false`；
//! 4. 换库真的把内容换回去，**原库连同 `-wal`/`-shm` 一起进留底目录**；
//! 5. 备份包不被消耗（恢复完还在，还能再恢复一次）；
//! 6. 换上去的库当场体检不过 → 原状回滚，一个字节都不少；
//! 7. 单个库文件（作者手选 / 命令行救援来的）也能恢复；
//! 8. 正在用的那个库不能当恢复源（拿它恢复它自己 = 什么都没做，还可能把库弄坏）。

use std::path::{Path, PathBuf};

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{
    list_packages, read_manifest, scan_packages, swap_in, BackupRequest, BackupTarget, Store,
    KEEP_FOLDER,
};

struct Fixture {
    _dir: tempfile::TempDir,
    data_dir: PathBuf,
    target: PathBuf,
    store: Store,
    chapter: i64,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let mut store = Store::open(data_dir.join("yanmo.db")).unwrap();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store
        .create_node(work.id, Some(volume), NodeKind::Chapter, "第一章")
        .unwrap();
    store.write_body(chapter, "雨下了整夜。").unwrap();
    let target = dir.path().join("备份盘");
    Fixture { _dir: dir, data_dir, target, store, chapter }
}

fn make_backup(f: &mut Fixture) -> PathBuf {
    let report = f
        .store
        .backup_now(&BackupRequest {
            data_dir: f.data_dir.clone(),
            targets: vec![BackupTarget {
                path: f.target.to_string_lossy().to_string(),
                volume_id: "vol-other".to_string(),
                volume_label: "备份盘".to_string(),
                removable: true,
            }],
            keep: 7,
            tz_offset_minutes: 480,
            device: "测试机".to_string(),
        })
        .unwrap();
    assert_eq!(report.succeeded(), 1, "备份该成功：{:?}", report.outcomes);
    PathBuf::from(&report.outcomes[0].package)
}

#[test]
fn backup_packages_are_listed_and_look_alike_directories_are_not() {
    let mut f = fixture();
    let package = make_backup(&mut f);
    // 别人的目录、以及"名字像但没有我们清单"的半截目录：都不该列出来
    std::fs::create_dir_all(f.target.join("我的照片")).unwrap();
    std::fs::create_dir_all(f.target.join("研墨备份-20200101-0000")).unwrap();

    let listed = list_packages(&f.target);
    assert_eq!(listed.len(), 1, "只该列出真备份：{listed:?}");
    assert_eq!(listed[0].path, package.to_string_lossy());
    assert_eq!(listed[0].device, "测试机");
    assert_eq!(listed[0].works, 1);
    assert_eq!(listed[0].chapters, 1);
    assert!(listed[0].words > 0, "清单里的字数要能报出来");
    assert!(listed[0].bytes > 0, "整包大小要能报出来");

    // 扫多个位置：去重之后还是一份；没有备份的目录给空列表
    let scanned = scan_packages(&[f.target.clone(), f.target.clone(), f.data_dir.clone()]);
    assert_eq!(scanned.len(), 1);
    assert!(list_packages(&f.data_dir).is_empty(), "数据目录里没有备份包");
    assert!(list_packages(&f._dir.path().join("没有这个目录")).is_empty());
}

#[test]
fn preview_reports_health_and_how_much_would_be_lost() {
    let mut f = fixture();
    let package = make_backup(&mut f);

    let preview = f.store.preview_restore(&package).unwrap();
    assert!(preview.verify.ok, "刚写完的包必须体检通过：{:?}", preview.verify.problems);
    assert!(preview.can_restore);
    assert_eq!(preview.kind, "package");
    assert_eq!(preview.source_works, 1);
    assert_eq!(preview.source_chapters, 1);
    assert_eq!(preview.lost_words, 0, "什么都没改：不该说会少字");
    assert_eq!(preview.lost_days, 0, "刚做完的备份：一天都不该退");
    assert!(preview.keep_dir.ends_with(KEEP_FOLDER), "{}", preview.keep_dir);
    assert!(preview.engine_version.contains('.'), "清单里的引擎版本要报给作者");
    assert!(read_manifest(&package).is_some(), "预览不该动包");

    // 备份之后又写了一大段 → 恢复会把这段退回去，预览必须如实说
    f.store.write_body(f.chapter, &"新写的段落。".repeat(80)).unwrap();
    let after = f.store.preview_restore(&package).unwrap();
    assert!(after.verify.ok);
    assert!(after.source_words < after.live_words);
    assert_eq!(after.lost_words, after.live_words - after.source_words);
    assert!(after.live_last_write_at >= after.source_last_write_at);
}

#[test]
fn a_damaged_package_can_never_be_restored_and_touches_nothing() {
    let mut f = fixture();
    let package = make_backup(&mut f);

    // ① 快照被改坏
    std::fs::write(package.join("yanmo.db"), "这不是一个 SQLite 库").unwrap();
    let preview = f.store.preview_restore(&package).unwrap();
    assert!(!preview.verify.ok && !preview.can_restore, "{:?}", preview.verify.problems);
    assert!(
        preview.verify.problems.iter().any(|problem| problem.contains("快照")),
        "要指到快照这一处：{:?}",
        preview.verify.problems
    );
    assert_eq!(f.store.read_body(f.chapter).unwrap(), "雨下了整夜。", "预览不该动活库");
}

#[test]
fn a_package_missing_its_draft_or_from_a_newer_engine_is_refused() {
    let mut f = fixture();
    let package = make_backup(&mut f);

    // ② 成稿少一个文件（不装研墨也能读的那一层没了）
    let draft = walk(&package.join("成稿"))
        .into_iter()
        .find(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .expect("成稿目录里该有分章文件");
    let saved = std::fs::read(&draft).unwrap();
    std::fs::remove_file(&draft).unwrap();
    let preview = f.store.preview_restore(&package).unwrap();
    assert!(!preview.can_restore, "成稿缺了就不算一份完整备份");
    assert!(preview.verify.problems.iter().any(|p| p.contains("成稿读不到")));
    std::fs::write(&draft, saved).unwrap();

    // ③ 清单说它来自更新的研墨：当前引擎别猜着读
    let manifest_path = package.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["data_format_version"] =
        serde_json::json!(yanmo_core::version::DATA_FORMAT_VERSION + 1);
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let preview = f.store.preview_restore(&package).unwrap();
    assert!(!preview.can_restore, "来自更新版本的备份不能硬恢复");
    assert!(preview.verify.problems.iter().any(|p| p.contains("更新的研墨")));
}

#[test]
fn restoring_swaps_the_content_and_keeps_the_old_database() {
    let mut f = fixture();
    let package = make_backup(&mut f);
    // 备份之后写的这一版：恢复之后就"丢"了，但原库要留底留得住
    f.store.write_body(f.chapter, "备份之后写的这一版。").unwrap();
    drop(f.store);

    let outcome = swap_in(&package, &f.data_dir, "20260913-1400").unwrap();
    assert!(outcome.restored_from.ends_with("yanmo.db"));
    assert!(outcome.quarantine.ends_with("20260913-1400"), "{}", outcome.quarantine);
    assert!(package.is_dir(), "备份包是恢复的来源，不该被消耗掉");

    let restored = Store::open(f.data_dir.join("yanmo.db")).unwrap();
    assert_eq!(
        restored.read_body(f.chapter).unwrap(),
        "雨下了整夜。",
        "换回来的该是备份里那一版"
    );
    drop(restored);

    // 留底那份还在，而且确实是"换之前"的那一版（打开它读得到新写的那句）
    let kept = Path::new(&outcome.quarantine).join("yanmo.db");
    assert!(kept.is_file(), "原库要留底：{}", kept.display());
    let old = Store::open(&kept).unwrap();
    assert_eq!(old.read_body(f.chapter).unwrap(), "备份之后写的这一版。");
}

#[test]
fn the_old_wal_and_shm_go_into_quarantine_with_the_old_database() {
    let mut f = fixture();
    let package = make_backup(&mut f);
    drop(f.store);
    // 崩溃现场就是这副样子：库旁边躺着 -wal / -shm
    std::fs::write(f.data_dir.join("yanmo.db-wal"), b"wal").unwrap();
    std::fs::write(f.data_dir.join("yanmo.db-shm"), b"shm").unwrap();

    let outcome = swap_in(&package, &f.data_dir, "20260913-1400").unwrap();
    let keep = Path::new(&outcome.quarantine);
    for name in ["yanmo.db", "yanmo.db-wal", "yanmo.db-shm"] {
        assert!(keep.join(name).is_file(), "{name} 该一起留底：{}", keep.display());
    }
    for name in ["yanmo.db-wal", "yanmo.db-shm"] {
        assert!(
            !f.data_dir.join(name).exists(),
            "{name} 不能留在数据目录——不一致的 WAL 留给新库比坏库更难查"
        );
    }
    assert!(f.data_dir.join("yanmo.db").is_file(), "新库待在数据目录里");
    assert!(!std::fs::read(f.data_dir.join("yanmo.db")).unwrap().is_empty());
}

#[test]
fn a_swap_that_fails_puts_everything_back_the_way_it_was() {
    let mut f = fixture();
    let package = make_backup(&mut f);
    f.store.write_body(f.chapter, "备份之后写的这一版。").unwrap();
    drop(f.store);
    let before = std::fs::read(f.data_dir.join("yanmo.db")).unwrap();

    // 一份打不开的"库"：复制得过去、也放得下，但当场体检一定不过 → 必须回滚
    let junk = f._dir.path().join("手选来的.db");
    std::fs::write(&junk, "这不是一个 SQLite 库").unwrap();
    let error = swap_in(&junk, &f.data_dir, "20260913-1400").unwrap_err();
    assert!(format!("{error}").contains("换库没成功"), "错误要说清是换库这一步：{error}");

    assert_eq!(
        std::fs::read(f.data_dir.join("yanmo.db")).unwrap(),
        before,
        "原库必须一个字节不少地回到原处"
    );
    assert!(!f.data_dir.join("yanmo.db.restoring").exists(), "临时文件要收掉");
    let kept = f.data_dir.join(KEEP_FOLDER).join("20260913-1400").join("yanmo.db");
    assert!(!kept.exists(), "回滚之后留底该搬回去了：{}", kept.display());
    assert!(package.is_dir(), "备份包还在");
}

#[test]
fn a_plain_database_file_picked_by_hand_can_be_restored_too() {
    let mut f = fixture();
    let package = make_backup(&mut f);
    let picked = f._dir.path().join("作者手选的.db");
    std::fs::copy(package.join("yanmo.db"), &picked).unwrap();

    let preview = f.store.preview_restore(&picked).unwrap();
    assert!(preview.can_restore, "{:?}", preview.verify.problems);
    assert_eq!(preview.kind, "database");
    assert_eq!(preview.source_words, preview.live_words, "刚备份完，两边该一样多");

    f.store.write_body(f.chapter, "备份之后写的这一版。").unwrap();
    drop(f.store);
    swap_in(&picked, &f.data_dir, "20260913-1500").unwrap();
    let restored = Store::open(f.data_dir.join("yanmo.db")).unwrap();
    assert_eq!(restored.read_body(f.chapter).unwrap(), "雨下了整夜。");
}

#[test]
fn the_database_you_are_using_is_not_a_restore_source() {
    let f = fixture();
    let live = f.data_dir.join("yanmo.db");
    let preview = f.store.preview_restore(&live).unwrap();
    assert!(preview.is_live_database, "选的正是活库，要认出来");
    assert!(!preview.can_restore, "拿活库恢复活库 = 什么都没做，还可能把库弄坏");
    assert!(preview.verify.ok, "不是它坏了，是它不该当来源");

    // 直接调换库也必须在动文件之前拦住
    let error = swap_in(&live, &f.data_dir, "20260913-1600").unwrap_err();
    assert!(format!("{error}").contains("不是一个备份包或库文件"), "{error}");
    assert!(live.is_file(), "活库还在原地");
}

/// 递归列出目录下的文件（成稿是按卷分目录的，不能只看一层）。
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
