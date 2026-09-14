//! 多处备份的验收：**作者能不能拿到一份真能用的备份**。
//!
//! 六条判据：
//! 1. 备份包里有：一致性快照 + 成稿导出 + 清单 + 账本副本；
//! 2. 写完**读回体检**通过才算成功；
//! 3. 体检**抓得住**被改坏的快照、被删掉的成稿（不然"体检"就是摆设）；
//! 4. 目标不可达 → 跳过并记账，**不静默补做**、不影响别的目标；
//! 5. 保留滚动只删自己写的旧包，别人的文件一个不碰；
//! 6. 链式指纹连得上上一份（中间少了一份能看出来）。

use std::path::{Path, PathBuf};

use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::store::{
    read_ledger, read_manifest, BackupConfig, BackupRequest, BackupTarget, Store,
};
use yanmo_core::time::now_millis;

struct Fixture {
    _dir: tempfile::TempDir,
    data_dir: PathBuf,
    target: PathBuf,
    store: Store,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let target = dir.path().join("备份盘");
    let store = Store::open(data_dir.join("yanmo.db")).unwrap();
    Fixture { _dir: dir, data_dir, target, store }
}

fn seed_book(store: &mut Store) {
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    for (index, body) in ["雨下了整夜。", "他把铜钱按在桌上。"].iter().enumerate() {
        let chapter = store
            .create_node(work.id, Some(volume), NodeKind::Chapter, &format!("第{}章", index + 1))
            .unwrap();
        store.write_body(chapter, body).unwrap();
    }
}

fn request(fixture: &Fixture, keep: usize) -> BackupRequest {
    BackupRequest {
        data_dir: fixture.data_dir.clone(),
        targets: vec![BackupTarget {
            path: fixture.target.to_string_lossy().to_string(),
            volume_id: "vol-other".to_string(),
            volume_label: "备份盘".to_string(),
            removable: true,
        }],
        keep,
        tz_offset_minutes: 480,
        device: "测试机".to_string(),
    }
}

/// 递归列出目录下的**文件**（备份包里的成稿是按卷分目录的，不能只看一层）。
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn packages(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

#[test]
fn a_backup_is_a_package_that_verifies() {
    let mut f = fixture();
    seed_book(&mut f.store);

    let report = f.store.backup_now(&request(&f, 7)).unwrap();
    assert_eq!(report.succeeded(), 1, "目标可达就该成功：{:?}", report.outcomes);
    let package = PathBuf::from(&report.outcomes[0].package);
    assert!(package.is_dir());

    // 包里该有的东西：快照 + 清单 + 成稿 + 账本副本
    assert!(package.join("yanmo.db").is_file(), "一致性快照要在包里");
    let manifest = read_manifest(&package).expect("清单要能读出来");
    assert_eq!(manifest.device, "测试机");
    assert_eq!(manifest.works.len(), 1);
    assert_eq!(manifest.works[0].chapters, 2, "两章");
    assert!(manifest.works[0].word_count > 0);
    assert!(package.join("backup-ledger.json").is_file(), "账本副本要在包里");
    assert!(
        manifest.works[0].files.iter().any(|f| f.ends_with(".txt")),
        "成稿（人不装研墨也能读的那份）要在包里"
    );

    // 读回体检：这就是"成功"的判据
    let verify = f.store.verify_backup(&package);
    assert!(verify.ok, "刚写完的包必须体检通过：{:?}", verify.problems);

    // 账本记了这一笔（库之外的那份）
    let ledger = read_ledger(&f.data_dir);
    let last = ledger.last_success(&f.target.to_string_lossy()).expect("账本要记成功");
    assert_eq!(last.fingerprint, manifest.fingerprint);
    assert!(!last.date.is_empty());
}

#[test]
fn verification_catches_a_broken_snapshot_or_a_missing_draft() {
    let mut f = fixture();
    seed_book(&mut f.store);
    let report = f.store.backup_now(&request(&f, 7)).unwrap();
    let package = PathBuf::from(&report.outcomes[0].package);
    assert!(f.store.verify_backup(&package).ok);

    // ① 成稿少一个文件 → 体检必须说话（成稿按卷分了目录，得递归找）
    let victim = files_under(&package.join("成稿"))
        .into_iter()
        .find(|p| p.extension().map(|e| e == "txt").unwrap_or(false))
        .expect("成稿目录里该有分章文件");
    let saved = std::fs::read(&victim).unwrap();
    std::fs::remove_file(&victim).unwrap();
    let verify = f.store.verify_backup(&package);
    assert!(!verify.ok, "删掉成稿之后体检不该还说过");
    assert!(
        verify.problems.iter().any(|p| p.contains("成稿读不到")),
        "要说清是哪一处出的问题：{:?}",
        verify.problems
    );
    std::fs::write(&victim, saved).unwrap();
    assert!(f.store.verify_backup(&package).ok, "还原之后又该是好的");

    // ② 快照被改坏 → 结构体检要抓出来
    let snapshot = package.join("yanmo.db");
    std::fs::write(&snapshot, "这不是一个 SQLite 库").unwrap();
    let verify = f.store.verify_backup(&package);
    assert!(!verify.ok, "快照坏掉必须被抓住");
    assert!(!verify.problems.is_empty());
}

#[test]
fn an_unreachable_target_is_skipped_and_recorded_without_blocking_others() {
    let mut f = fixture();
    seed_book(&mut f.store);

    // 造一个"永远建不出来的目标"：父级是一个**文件**（跨平台都成立）
    let blocker = f._dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let mut request = request(&f, 7);
    request.targets.push(BackupTarget {
        path: blocker.join("备份").to_string_lossy().to_string(),
        volume_id: "vol-gone".to_string(),
        volume_label: "拔掉的盘".to_string(),
        removable: true,
    });

    let report = f.store.backup_now(&request).unwrap();
    assert_eq!(report.succeeded(), 1, "好目标照样成功");
    assert_eq!(report.skipped(), 1, "拔掉的盘算跳过，不算失败也不静默补做");
    let skipped = report.outcomes.iter().find(|o| o.status == "skipped").unwrap();
    assert!(
        skipped.reason.contains("写不进去"),
        "父级是个文件、盘根却在——原因要如实说「写不进去」，别赖给「盘没插」：{}",
        skipped.reason
    );

    // 跳过也记账（"缺了哪几天"靠它）
    let ledger = read_ledger(&f.data_dir);
    let attempt = ledger.last_attempt(&blocker.join("备份").to_string_lossy()).unwrap();
    assert_eq!(attempt.status, "skipped");
}

#[test]
fn retention_keeps_the_newest_and_never_touches_foreign_files() {
    let mut f = fixture();
    seed_book(&mut f.store);
    std::fs::create_dir_all(&f.target).unwrap();
    // 别人的东西：一个目录 + 一个文件，都不该被清理碰到
    std::fs::create_dir_all(f.target.join("我的照片")).unwrap();
    std::fs::write(f.target.join("说明书.txt"), "别删我".as_bytes()).unwrap();

    for _ in 0..3 {
        let report = f.store.backup_now(&request(&f, 2)).unwrap();
        assert_eq!(report.succeeded(), 1);
    }
    let ours = packages(&f.target)
        .into_iter()
        .filter(|p| read_manifest(p).is_some())
        .collect::<Vec<_>>();
    assert_eq!(ours.len(), 2, "保留 2 份：{:?}", ours);
    assert!(f.target.join("我的照片").is_dir(), "别人的目录不许删");
    assert!(f.target.join("说明书.txt").is_file(), "别人的文件不许删");
}

#[test]
fn the_chain_links_each_backup_to_the_previous_one() {
    let mut f = fixture();
    seed_book(&mut f.store);

    let first = f.store.backup_now(&request(&f, 7)).unwrap();
    let first_manifest = read_manifest(Path::new(&first.outcomes[0].package)).unwrap();
    assert!(first_manifest.previous_fingerprint.is_none(), "第一份没有上一环");

    let second = f.store.backup_now(&request(&f, 7)).unwrap();
    assert_eq!(second.succeeded(), 1, "第二份也该成功：{:?}", second.outcomes);
    let second_manifest = read_manifest(Path::new(&second.outcomes[0].package))
        .unwrap_or_else(|| panic!("第二份的包读不出来：{:?}", second.outcomes));
    assert_eq!(
        second_manifest.previous_fingerprint.as_deref(),
        Some(first_manifest.fingerprint.as_str()),
        "第二份要连上第一份——中间少一份就能看出来"
    );
}

#[test]
fn a_failed_snapshot_is_reported_per_target_instead_of_blowing_up() {
    let mut f = fixture();
    seed_book(&mut f.store);
    // 暂存区被一个**文件**占住 → 快照写不出来：报告里每个目标都该被记失败，而不是 panic
    std::fs::create_dir_all(&f.data_dir).unwrap();
    let stamp = yanmo_core::time::local_stamp(now_millis(), 480);
    std::fs::write(f.data_dir.join(format!(".backup-staging-{stamp}")), "占位".as_bytes()).unwrap();

    let report = f.store.backup_now(&request(&f, 7)).unwrap();
    assert_eq!(report.failed(), 1, "报告：{:?}", report.outcomes);
    assert!(report.outcomes[0].reason.contains("快照"), "原因要说清是快照做不出来：{}", report.outcomes[0].reason);
}

#[test]
fn config_round_trips_and_defaults_are_sane() {
    let mut f = fixture();
    let defaults = f.store.backup_config().unwrap();
    assert_eq!(defaults, BackupConfig::default());
    assert!(defaults.targets.is_empty(), "默认一个目标都不预勾");
    assert_eq!(defaults.keep, 7, "默认保留 7 份");
    assert!(defaults.auto_on_start && defaults.auto_on_close, "自动两条默认开（没目标时什么都不做）");

    let wanted = BackupConfig {
        targets: vec![BackupTarget {
            path: "备份盘/研墨备份".to_string(),
            volume_id: "1234ABCD".to_string(),
            volume_label: "备份盘".to_string(),
            removable: false,
        }],
        keep: 3,
        auto_on_start: false,
        auto_on_close: true,
        ..BackupConfig::default()
    };
    f.store.set_backup_config(&wanted).unwrap();
    assert_eq!(f.store.backup_config().unwrap(), wanted);
    assert!(
        yanmo_core::store::has_other_volume(&wanted, "本机数据卷"),
        "勾了异盘目标就不该再提醒插盘"
    );
    assert!(
        !yanmo_core::store::has_other_volume(&wanted, "1234ABCD"),
        "目标就是数据所在的卷时，仍算「没有异盘目标」"
    );
}

#[test]
fn every_book_gets_its_own_draft_file() {
    // 两本书的成稿**不能互相覆盖**：备份包是"不装研墨也能读"的那份保险，
    // 一份包里出现两个 work.json 时，后写的会把先写的顶掉——静默丢一本书。
    let mut f = fixture();
    seed_book(&mut f.store);
    let second = f.store.create_work(WorkKind::Novel, "短歌").unwrap();
    let volume = f.store.list_nodes(second.id).unwrap()[0].id;
    let chapter = f
        .store
        .create_node(second.id, Some(volume), NodeKind::Chapter, "第一章")
        .unwrap();
    f.store.write_body(chapter, "短歌的正文。").unwrap();

    let report = f.store.backup_now(&request(&f, 7)).unwrap();
    assert_eq!(report.succeeded(), 1, "报告：{:?}", report.outcomes);
    let package = PathBuf::from(&report.outcomes[0].package);
    let manifest = read_manifest(&package).expect("清单要能读出来");
    assert_eq!(manifest.works.len(), 2, "两本书");

    // 逐书：清单里列的每个文件都必须真的在包里，且 JSON 成稿里的书名要对得上
    for work in &manifest.works {
        let json = work.files.iter().find(|f| f.ends_with(".json")).expect("每本书都该有 JSON 成稿");
        let text = std::fs::read_to_string(package.join(json)).expect("清单里列的文件必须在包里");
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["title"], work.title, "《{}》的成稿文件（{json}）里是别人的书", work.title);
    }
    assert!(f.store.verify_backup(&package).ok, "体检仍应通过");
}
