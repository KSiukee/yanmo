//! 「稿子放在哪」的验收：位置记录、老位置认领、首启推荐、搬家与核对。
//!
//! 这一组测试盯的是**作者最怕的那件事**：换个地方放稿子，结果稿子不见了。
//! 所以每条都问同一个问题——"出这种事的时候，代码会不会静默地换个地方/删掉东西"。

use std::path::{Path, PathBuf};

use yanmo_core::location::{self, DirSource, Reason, Risk, Sources};
use yanmo_core::model::{NodeKind, WorkKind};
use yanmo_core::paths::{self, DB_FILE};
use yanmo_core::store::{self, Store};

/// 造一份"有内容"的稿库（只有有内容，规模账的核对才有意义）。
fn seed(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let mut store = Store::open(dir.join(DB_FILE)).unwrap();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let root = store.list_nodes(work.id).unwrap()[0].id;
    let chapter = store.create_node(work.id, Some(root), NodeKind::Chapter, "第一章").unwrap();
    store.write_body(chapter, "第一章的正文。").unwrap();
}

/// 一台"干净的机器"：程序目录、系统数据目录、文档、主目录都在临时目录里。
fn machine(root: &Path) -> Sources {
    let sources = Sources {
        exe_dir: root.join("program"),
        system_dir: Some(root.join("appdata/app.yanmo.desktop")),
        documents: Some(root.join("documents")),
        desktop: Some(root.join("documents/Desktop")),
        profile: Some(root.join("home")),
        synced_roots: vec![root.join("onedrive")],
    };
    for dir in [&sources.exe_dir, sources.documents.as_ref().unwrap(), sources.profile.as_ref().unwrap()] {
        std::fs::create_dir_all(dir).unwrap();
    }
    sources
}

#[test]
fn a_fresh_machine_is_a_first_run_and_recommends_the_documents_folder() {
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    let found = location::resolve(&sources).expect("推荐位置应当拼得出来");
    assert_eq!(found.source, DirSource::FirstRun);
    assert!(found.source.is_first_run());
    assert_eq!(found.dir, root.path().join("documents").join("研墨"));

    let suggestion = location::suggest(&sources).unwrap();
    assert_eq!(suggestion.reason, Reason::Documents, "文档可用时该推荐文档");
    assert_eq!(suggestion.reason.code(), "documents");
}

#[test]
fn the_record_wins_even_when_the_folder_is_not_there_right_now() {
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    let pointer = root.path().join("appdata/app.yanmo.desktop").join(paths::LOCATION_FILE);
    // 作者选过的位置：现在没插盘（目录不存在）
    let chosen = root.path().join("usb/我的稿子");
    location::write_record(&pointer, &chosen).unwrap();

    let found = location::resolve(&sources).unwrap();
    assert_eq!(found.source, DirSource::Recorded);
    assert_eq!(found.dir, chosen, "记录说了算——拔盘时也不能自己换地方");
    assert!(!found.source.is_first_run(), "作者选过就不算首启");
    assert_eq!(location::read_record(&pointer), Some(chosen));
}

#[test]
fn a_broken_record_counts_as_no_record() {
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    let pointer = root.path().join("appdata/app.yanmo.desktop").join(paths::LOCATION_FILE);
    std::fs::create_dir_all(pointer.parent().unwrap()).unwrap();

    for garbage in ["", "   \n", "# 只有注释\n", "相对路径/我的稿子\n", "随便一句话\n"] {
        std::fs::write(&pointer, garbage).unwrap();
        let found = location::resolve(&sources).unwrap();
        assert_eq!(
            found.source,
            DirSource::FirstRun,
            "读不懂的记录要当没记录，绝不能拿它把稿子建到别处：{garbage:?}"
        );
        assert_eq!(found.dir, root.path().join("documents").join("研墨"));
    }
}

#[test]
fn an_old_database_is_adopted_instead_of_telling_the_author_it_is_the_first_time() {
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    // 老版本把库放在系统数据目录里——升上来的作者不该看见"首次使用"
    let old = sources.system_dir.clone().unwrap();
    seed(&old);

    let found = location::resolve(&sources).unwrap();
    assert_eq!(found.source, DirSource::LegacySystem);
    assert_eq!(found.dir, old);
    assert!(!found.source.is_first_run(), "老作者不算首启");
}

#[test]
fn a_portable_build_adopts_the_library_next_to_the_program_and_recommends_it_too() {
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    std::fs::write(sources.exe_dir.join(paths::PORTABLE_MARKER), b"portable").unwrap();
    let beside = sources.exe_dir.join(paths::PORTABLE_DATA_FOLDER);

    // 先看推荐：绿色包应当推荐程序旁（否则"绿色"就成了把稿子写进 C 盘）
    assert_eq!(location::suggest(&sources).unwrap().reason, Reason::Portable);

    // 再看认领：程序旁已经有库 → 认领它
    seed(&beside);
    let found = location::resolve(&sources).unwrap();
    assert_eq!(found.source, DirSource::LegacyPortable);
    assert_eq!(found.dir, beside);
}

#[test]
fn a_synced_documents_folder_pushes_the_suggestion_to_the_home_folder() {
    let root = tempfile::tempdir().unwrap();
    let mut sources = machine(root.path());
    let synced = root.path().join("onedrive/文档");
    std::fs::create_dir_all(&synced).unwrap();
    sources.documents = Some(synced.clone());

    let suggestion = location::suggest(&sources).unwrap();
    assert_eq!(
        suggestion.reason,
        Reason::DocumentsSynced,
        "文档被 OneDrive 接管时不能推荐它——同步盘上跑库会撞坏稿子"
    );
    assert_eq!(suggestion.dir, root.path().join("home").join("研墨"));
}

#[test]
fn a_portable_copy_never_adopts_the_machines_system_folder() {
    // 这一条是踩出来的：绿色版插到一台装过研墨的机器上，如果去"认领"那台机器的
    // `%APPDATA%`，作者会以为自己的稿子被搬走了——而绿色版本该只动自己文件夹里的东西。
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    std::fs::write(sources.exe_dir.join(paths::PORTABLE_MARKER), b"portable").unwrap();
    // 这台机器上确实有一份安装版的库（系统数据目录里）
    seed(&sources.system_dir.clone().unwrap());

    let found = location::resolve(&sources).unwrap();
    assert_eq!(
        found.source,
        DirSource::FirstRun,
        "便携版没有记录时该走首启推荐，绝不能去认领本机的系统数据目录"
    );
    assert_eq!(found.dir, sources.exe_dir.join(paths::PORTABLE_DATA_FOLDER));
    assert!(
        !found.dir.starts_with(sources.system_dir.as_ref().unwrap()),
        "定下来的位置不该落在系统数据目录里：{}",
        found.dir.display()
    );

    // 反过来：便携版自己程序旁的 data/ 里有库时，认领它是应该的
    seed(&sources.exe_dir.join(paths::PORTABLE_DATA_FOLDER));
    let adopted = location::resolve(&sources).unwrap();
    assert_eq!(adopted.source, DirSource::LegacyPortable);
}

#[test]
fn the_risk_list_flags_the_places_that_bite() {
    let root = tempfile::tempdir().unwrap();
    let sources = machine(root.path());
    let current = root.path().join("documents/研墨");

    let synced = root.path().join("onedrive/稿子");
    assert!(location::risks(&synced, &sources, &current).contains(&Risk::Synced));

    let desktop = root.path().join("documents/Desktop/稿子");
    assert!(location::risks(&desktop, &sources, &current).contains(&Risk::Desktop));

    // 数据目录里面（搬迁会被拒：不能往自己里面搬）
    let inside = current.join("备份/稿子");
    assert!(location::risks(&inside, &sources, &current).contains(&Risk::InsideData));

    // 已经有一份库
    let occupied = root.path().join("documents/另一份");
    seed(&occupied);
    assert!(location::risks(&occupied, &sources, &current).contains(&Risk::Occupied));

    // 干净的落点：一条风险都不该有
    let clean = root.path().join("documents/新位置");
    assert!(location::risks(&clean, &sources, &current).is_empty(), "干净的落点不该报风险");
}

#[test]
fn relocation_copies_everything_and_refuses_the_three_dangerous_moves() {
    let root = tempfile::tempdir().unwrap();
    let from = root.path().join("documents/研墨");
    seed(&from);
    std::fs::create_dir_all(from.join("escape")).unwrap();
    std::fs::write(from.join("escape/救回来的.md"), "正文").unwrap();
    let target = root.path().join("D/我的稿子");

    let report = store::copy_dir(&from, &target).unwrap();
    assert_eq!(report.dir, target);
    assert!(report.files >= 2, "库与逃生目录都该复制过去：{report:?}");
    assert!(target.join(DB_FILE).is_file());
    assert_eq!(std::fs::read_to_string(target.join("escape/救回来的.md")).unwrap(), "正文");
    assert!(from.join(DB_FILE).is_file(), "旧位置一个字都不能少");

    // ① 原位置没有库
    let empty = root.path().join("什么都没有");
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        store::copy_dir(&empty, &root.path().join("X")).unwrap_err().code(),
        yanmo_core::error_codes::codes::STORE_RELOCATE_SOURCE_MISSING
    );
    // ② 往自己里面搬
    assert_eq!(
        store::copy_dir(&from, &from.join("子目录")).unwrap_err().code(),
        yanmo_core::error_codes::codes::STORE_RELOCATE_INSIDE
    );
    // ③ 新位置已经有一份稿子
    assert_eq!(
        store::copy_dir(&from, &target).unwrap_err().code(),
        yanmo_core::error_codes::codes::STORE_RELOCATE_TARGET_IN_USE
    );
}

#[test]
fn relocation_verification_accepts_a_faithful_copy_and_catches_a_broken_one() {
    let root = tempfile::tempdir().unwrap();
    let from = root.path().join("documents/研墨");
    seed(&from);
    let target = root.path().join("D/我的稿子");
    store::copy_dir(&from, &target).unwrap();
    store::verify_same_scale(&target, &from).expect("一模一样的复制必须核对通过");

    // 把复制过去的那份截断：核对必须当场拦住（这就是"复制没成"的样子）
    let broken = root.path().join("D/坏副本");
    store::copy_dir(&from, &broken).unwrap();
    std::fs::write(broken.join(DB_FILE), "这不是一个数据库".as_bytes()).unwrap();
    let error = store::verify_same_scale(&broken, &from).unwrap_err();
    assert_eq!(error.code(), yanmo_core::error_codes::codes::STORE_RELOCATE_VERIFY_FAILED);
    assert_eq!(
        error.params().iter().find(|(name, _)| *name == "path").map(|(_, value)| value.clone()),
        Some(broken.display().to_string())
    );
}

#[test]
fn a_hand_written_record_still_reads_back() {
    let root = tempfile::tempdir().unwrap();
    let pointer: PathBuf = root.path().join("yanmo-location.txt");
    let dir = root.path().join("稿子");
    location::write_record(&pointer, &dir).unwrap();
    // 作者拿记事本打开、把路径加了引号、前面留了空行：照样读得出来
    std::fs::write(&pointer, format!("\n\n  # 手改过的\n\"{}\"  \n", dir.display())).unwrap();
    assert_eq!(location::read_record(&pointer), Some(dir));
}
