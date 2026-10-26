//! 认证验真验收：**签名清单 + 载荷哈希锚定**；没签名 / 改一行 / 换哈希都不算。
//!
//! 这里用测试密钥签；官方私钥永远不进仓库。

use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use tempfile::TempDir;
use yanmo_core::error_codes::codes;
use yanmo_core::store::{
    inspect_module_package_with_certs, Certification, CertificationSet, Store,
};
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};

const ID: &str = "app.example.signed";

fn manifest() -> String {
    json!({
        "id": ID,
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "签名测试" },
        "description": { "zh-Hans": "认证验收用。" },
        "author": { "name": "Example Studio" },
        "version": "2.0.0",
        "protocol": 1,
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "headless" },
        "capabilities": { "read": ["works"] }
    })
    .to_string()
}

fn package(dir: &Path) -> PathBuf {
    let module = dir.join("module");
    fs::create_dir(&module).unwrap();
    fs::write(module.join("manifest.json"), manifest()).unwrap();
    fs::write(module.join("module.wasm"), wat::parse_str("(module)").unwrap()).unwrap();
    module
}

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn public_hex(signing: &SigningKey) -> String {
    signing.verifying_key().to_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn signed_json(entries: &[Certification], signing: &SigningKey) -> String {
    let message = CertificationSet::canonical_message(entries);
    let signature: ed25519_dalek::Signature = signing.sign(message.as_bytes());
    let signature_hex: String =
        signature.to_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
    json!({
        "format": 1,
        "entries": entries,
        "signature": signature_hex,
    })
    .to_string()
}

fn certification(hash: &str) -> Certification {
    Certification {
        module_id: ID.to_string(),
        version: "2.0.0".to_string(),
        payload_sha256: hash.to_string(),
        edition: 3,
        certified_at: 1_790_000_000_000,
    }
}

fn grant() -> ModuleCapabilities {
    ModuleCapabilities {
        read: vec![CoreDataArea::Works],
        ..ModuleCapabilities::none()
    }
}

#[test]
fn a_signed_certification_binds_to_the_payload_hash() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path());
    let preview = inspect_module_package_with_certs(&module, &CertificationSet::empty()).unwrap();
    let entries = vec![certification(&preview.fingerprint)];
    let set = CertificationSet::from_signed_json(
        &signed_json(&entries, &key()),
        &public_hex(&key()),
    )
    .unwrap();

    let certified = inspect_module_package_with_certs(&module, &set).unwrap();
    assert!(certified.certified);
    assert_eq!(certified.cert_edition, 3);
    assert_eq!(certified.certified_at, 1_790_000_000_000);
    assert_eq!(
        set.lookup(ID, "2.0.0", &preview.fingerprint).unwrap().date_text(),
        "2026-09-21"
    );
    // 换一个哈希（比如新版本）就不算
    assert!(set.lookup(ID, "2.0.0", &"0".repeat(64)).is_none());
    assert!(set.lookup(ID, "2.0.1", &preview.fingerprint).is_none());
}

#[test]
fn import_stores_the_verified_certification() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path());
    let preview = inspect_module_package_with_certs(&module, &CertificationSet::empty()).unwrap();
    let set = CertificationSet::from_signed_json(
        &signed_json(&[certification(&preview.fingerprint)], &key()),
        &public_hex(&key()),
    )
    .unwrap();

    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    store
        .import_module_with_certs(
            &module,
            &grant(),
            &preview.fingerprint,
            "folder",
            &set,
        )
        .unwrap();
    let card = &store.module_cards().unwrap()[0];
    assert!(card.certified);
    assert_eq!(card.cert_edition, 3);
    assert_eq!(card.certified_at, 1_790_000_000_000);
}

#[test]
fn a_tampered_entry_breaks_the_signature() {
    let entries = vec![Certification {
        module_id: ID.to_string(),
        version: "2.0.0".to_string(),
        payload_sha256: "a".repeat(64),
        edition: 1,
        certified_at: 1_790_000_000_000,
    }];
    let mut value: Value =
        serde_json::from_str(&signed_json(&entries, &key())).unwrap();
    value["entries"][0]["edition"] = json!(2);
    let error = CertificationSet::from_signed_json(
        &value.to_string(),
        &public_hex(&key()),
    )
    .unwrap_err();
    assert_eq!(error.code(), codes::MODULE_CERT_INVALID);
}

#[test]
fn a_wrong_key_or_wrong_format_is_rejected() {
    let entries = vec![Certification {
        module_id: ID.to_string(),
        version: "2.0.0".to_string(),
        payload_sha256: "b".repeat(64),
        edition: 1,
        certified_at: 1_790_000_000_000,
    }];
    let json = signed_json(&entries, &key());
    let wrong = SigningKey::from_bytes(&[8u8; 32]);
    let error =
        CertificationSet::from_signed_json(&json, &public_hex(&wrong)).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_CERT_INVALID);

    let mut value: Value = serde_json::from_str(&json).unwrap();
    value["format"] = json!(2);
    let error =
        CertificationSet::from_signed_json(&value.to_string(), &public_hex(&key())).unwrap_err();
    assert_eq!(error.code(), codes::MODULE_CERT_INVALID);
}

/// 认证条目带着"验过什么"：版本号决定清单，认不出的版本不假装认识。
#[test]
fn a_certification_says_what_was_checked() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path());
    let preview = inspect_module_package_with_certs(&module, &CertificationSet::empty()).unwrap();
    let mut entry = certification(&preview.fingerprint);
    entry.edition = 1;
    let set =
        CertificationSet::from_signed_json(&signed_json(&[entry], &key()), &public_hex(&key()))
            .unwrap();
    let checks = set.lookup(ID, "2.0.0", &preview.fingerprint).unwrap().checks();
    assert_eq!(checks.len(), 8);
    assert!(checks.contains(&"bench_all_scenarios"));
    assert!(checks.contains(&"network_via_proxy"));

    let mut future = certification(&preview.fingerprint);
    future.edition = 99;
    let set =
        CertificationSet::from_signed_json(&signed_json(&[future], &key()), &public_hex(&key()))
            .unwrap();
    assert!(
        set.lookup(ID, "2.0.0", &preview.fingerprint).unwrap().checks().is_empty(),
        "an unknown edition must not invent checks"
    );
}

/// 启动复核：清单里没了就撤销、回来了就恢复；没变就什么都不报。
#[test]
fn a_startup_recheck_revokes_and_restores() {
    let dir = TempDir::new().unwrap();
    let module = package(dir.path());
    let preview = inspect_module_package_with_certs(&module, &CertificationSet::empty()).unwrap();

    let mut store = Store::open(dir.path().join("yanmo.db")).unwrap();
    let grant = ModuleCapabilities {
        read: vec![CoreDataArea::Works],
        ..ModuleCapabilities::none()
    };
    let managed = dir.path().join("modules").join(ID);
    store
        .import_module_managed(&module, &managed, &grant, "", "folder", &CertificationSet::empty())
        .unwrap();
    // 模拟"装的时候是认证过的"（安装那一刻清单里有它）
    store.set_module_certified(ID, true).unwrap();

    assert!(store.module_card(ID).unwrap().certified, "前置：装好后是认证态");
    assert!(
        store.module_cards().unwrap().iter().any(|card| card.id == ID && card.certified),
        "列表也要看得到认证态"
    );
    // 现在的官方清单是空的：复核应当撤销
    let recheck = store.recheck_certifications_with(&CertificationSet::empty()).unwrap();
    assert_eq!(recheck.revoked, vec![ID.to_string()]);
    assert!(recheck.restored.is_empty());
    assert!(!store.module_card(ID).unwrap().certified, "撤销要落到记录上");

    // 再跑一次：已经一致，不该再报
    let again = store.recheck_certifications_with(&CertificationSet::empty()).unwrap();
    assert!(again.is_empty());

    // 清单里又有它了：应当恢复
    let entry = certification(&preview.fingerprint);
    let set = CertificationSet::from_signed_json(
        &signed_json(&[entry], &key()),
        &public_hex(&key()),
    )
    .unwrap();
    let restored = store.recheck_certifications_with(&set).unwrap();
    assert_eq!(restored.restored, vec![ID.to_string()]);
    assert!(store.module_card(ID).unwrap().certified);
}
