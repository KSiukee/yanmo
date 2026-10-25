//! 模块升级验收：**同 id 新版本**——预览说清升/降、升级保留偏好与数据、降级一律拒。
//!
//! 这一组只测核心的账与包：界面怎么问、怎么勾，是壳那一层的事。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::store::{CertificationSet, Store};
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};

const ID: &str = "app.example.upgradable";

fn manifest_json(version: &str, reads: &[&str], network: bool) -> String {
    json!({
        "id": ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "可升级模块" },
        "description": { "zh-Hans": "先装上，再升级。" },
        "author": { "name": "Example Studio" },
        "version": version,
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": {
            "read": reads,
            "own_data": true,
            "network": network
        }
    })
    .to_string()
}

fn package(root: &Path, name: &str, version: &str, reads: &[&str], network: bool) -> PathBuf {
    let module = root.join(name);
    fs::create_dir_all(&module).unwrap();
    fs::write(module.join("manifest.json"), manifest_json(version, reads, network)).unwrap();
    fs::write(module.join("module.wasm"), wat::parse_str("(module)").unwrap()).unwrap();
    module
}

fn grant(reads: &[CoreDataArea], network: bool) -> ModuleCapabilities {
    ModuleCapabilities {
        read: reads.to_vec(),
        own_data: true,
        network,
        ..ModuleCapabilities::none()
    }
}

fn fresh() -> (TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn the_preview_says_it_is_an_upgrade_and_what_is_new() {
    let (dir, mut store) = fresh();
    let managed = dir.path().join("modules").join(ID);
    let first = package(dir.path(), "v1", "1.0.0", &["works"], false);
    store
        .import_module_managed(
            &first,
            &managed,
            &grant(&[CoreDataArea::Works], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();

    // 新版本多要了正文与联网
    let second = package(dir.path(), "v2", "1.1.0", &["works", "body"], true);
    let preview = store.inspect_import(&second).unwrap();
    assert!(preview.installed);
    assert_eq!(preview.installed_version, "1.0.0");
    assert_eq!(preview.installed_description, "先装上，再升级。", "旧说明要带给界面做对比");
    assert!(preview.upgrade, "1.1.0 比 1.0.0 新");
    assert!(!preview.downgrade);
    assert_eq!(preview.added, vec!["read.body".to_string(), "network".to_string()]);
    assert_eq!(preview.granted.read, vec![CoreDataArea::Works], "现在只给了作品");
}

#[test]
fn upgrading_keeps_preferences_and_swaps_the_package() {
    let (dir, mut store) = fresh();
    let managed = dir.path().join("modules").join(ID);
    let first = package(dir.path(), "v1", "1.0.0", &["works"], false);
    store
        .import_module_managed(
            &first,
            &managed,
            &grant(&[CoreDataArea::Works], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();
    store.set_module_favorite(ID, true).unwrap();
    store.set_module_pinned(ID, true).unwrap();
    store.set_module_background(ID, true).unwrap();
    let before = store.module_card(ID).unwrap();

    let second = package(dir.path(), "v2", "2.0.0", &["works", "body"], false);
    // 新版本连载荷也换了（指纹跟着变）——这样才证明磁盘上那份真换了
    fs::write(second.join("module.wasm"), wat::parse_str("(module (memory 1))").unwrap()).unwrap();
    let receipt = store
        .upgrade_module_managed(
            &second,
            &managed,
            &dir.path().join("module-archive"),
            &grant(&[CoreDataArea::Works, CoreDataArea::Body], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();
    assert_eq!(receipt.from_version, "1.0.0");
    assert_eq!(receipt.to_version, "2.0.0");
    assert_eq!(receipt.added, vec!["read.body".to_string()]);

    let after = store.module_card(ID).unwrap();
    assert_eq!(after.version, "2.0.0");
    assert!(after.favorite && after.pinned && after.background, "偏好跟着走");
    assert_eq!(after.installed_at, before.installed_at, "装的时间不动");
    assert!(after.updated_at >= before.updated_at);
    assert_ne!(after.fingerprint, before.fingerprint);
    // 受管目录里现在是新包，启动前指纹复核过得去
    store.verify_module_package(ID, &managed).unwrap();
    let text = fs::read_to_string(managed.join("manifest.json")).unwrap();
    assert!(text.contains("2.0.0"), "磁盘上的包也换代了");
}

#[test]
fn a_same_or_older_version_is_refused_and_the_disk_is_untouched() {
    let (dir, mut store) = fresh();
    let managed = dir.path().join("modules").join(ID);
    let first = package(dir.path(), "v1", "1.2.0", &["works"], false);
    store
        .import_module_managed(
            &first,
            &managed,
            &grant(&[CoreDataArea::Works], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();
    let before = store.module_card(ID).unwrap();

    for version in ["1.2.0", "1.1.9"] {
        let same_or_older = package(dir.path(), &format!("pkg-{version}"), version, &["works"], false);
        let error = store
            .upgrade_module_managed(
                &same_or_older,
                &managed,
                &dir.path().join("module-archive"),
                &grant(&[CoreDataArea::Works], false),
                "",
                "folder",
                &CertificationSet::built_in(),
            )
            .unwrap_err();
        assert_eq!(error.code(), codes::MODULE_UPGRADE_NOT_NEWER, "{version}");
    }

    let after = store.module_card(ID).unwrap();
    assert_eq!(after.version, before.version);
    assert_eq!(after.fingerprint, before.fingerprint);
    store.verify_module_package(ID, &managed).unwrap();
}

#[test]
fn an_upgrade_cannot_grant_capabilities_the_new_manifest_did_not_declare() {
    let (dir, mut store) = fresh();
    let managed = dir.path().join("modules").join(ID);
    let first = package(dir.path(), "v1", "1.0.0", &["works"], false);
    store
        .import_module_managed(
            &first,
            &managed,
            &grant(&[CoreDataArea::Works], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();

    let second = package(dir.path(), "v2", "1.1.0", &["works"], false);
    let error = store
        .upgrade_module_managed(
            &second,
            &managed,
            &dir.path().join("module-archive"),
            // 新清单没声明"目录"，却想给"目录"——按"授予不得越过声明"拒
            &grant(&[CoreDataArea::Works, CoreDataArea::Tree], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_GRANT_NOT_DECLARED);
    assert_eq!(store.module_card(ID).unwrap().version, "1.0.0", "被拒时账不动");
}

/// 回滚：升级时留的档能把包、版本与**权限**一起退回去；回滚是一次性的。
#[test]
fn an_upgrade_can_be_rolled_back_once() {
    let (dir, mut store) = fresh();
    let managed = dir.path().join("modules").join(ID);
    let archive = dir.path().join("module-archive");
    let first = package(dir.path(), "v1", "1.0.0", &["works"], false);
    store
        .import_module_managed(
            &first,
            &managed,
            &grant(&[CoreDataArea::Works], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();
    let first_fingerprint = store.module_card(ID).unwrap().fingerprint;

    let second = package(dir.path(), "v2", "2.0.0", &["works", "body"], false);
    fs::write(second.join("module.wasm"), wat::parse_str("(module (memory 1))").unwrap()).unwrap();
    let receipt = store
        .upgrade_module_managed(
            &second,
            &managed,
            &archive,
            &grant(&[CoreDataArea::Works, CoreDataArea::Body], false),
            "",
            "folder",
            &CertificationSet::built_in(),
        )
        .unwrap();
    assert!(receipt.archived, "升级应当把上一版留成档");
    assert_eq!(store.module_record(ID).unwrap().granted.read.len(), 2);

    let back = store
        .rollback_module_managed(&archive, &managed, ID, &CertificationSet::built_in())
        .unwrap();
    assert_eq!(back.from_version, "2.0.0");
    assert_eq!(back.to_version, "1.0.0");
    let card = store.module_card(ID).unwrap();
    assert_eq!(card.version, "1.0.0");
    assert_eq!(card.fingerprint, first_fingerprint, "包也退回去了");
    assert_eq!(
        store.module_record(ID).unwrap().granted.read,
        vec![CoreDataArea::Works],
        "权限跟着退回那一版"
    );
    store.verify_module_package(ID, &managed).unwrap();

    // 一次性：留档已经收掉，再回滚就没了
    let error = store
        .rollback_module_managed(&archive, &managed, ID, &CertificationSet::built_in())
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_ROLLBACK_UNAVAILABLE);
}

/// 留档被动过（载荷换了）→ 拒，账与包都不动。
#[test]
fn a_tampered_archive_is_refused() {
    let (dir, mut store) = fresh();
    let managed = dir.path().join("modules").join(ID);
    let archive = dir.path().join("module-archive");
    let first = package(dir.path(), "v1", "1.0.0", &["works"], false);
    store
        .import_module_managed(&first, &managed, &grant(&[CoreDataArea::Works], false), "", "folder",
            &CertificationSet::built_in())
        .unwrap();
    let second = package(dir.path(), "v2", "2.0.0", &["works"], false);
    fs::write(second.join("module.wasm"), wat::parse_str("(module (memory 2))").unwrap()).unwrap();
    store
        .upgrade_module_managed(&second, &managed, &archive, &grant(&[CoreDataArea::Works], false),
            "", "folder", &CertificationSet::built_in())
        .unwrap();

    // 把留档里的载荷换掉
    let archived_wasm = archive.join(ID).join("1.0.0").join("package").join("module.wasm");
    fs::write(&archived_wasm, wat::parse_str("(module (memory 3))").unwrap()).unwrap();

    let error = store
        .rollback_module_managed(&archive, &managed, ID, &CertificationSet::built_in())
        .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_PACKAGE_TAMPERED);
    assert_eq!(store.module_card(ID).unwrap().version, "2.0.0", "被拒时账不动");
}
