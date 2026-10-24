//! 模块清单验收：**一份清单能写对、写错会被当场拒**。
//!
//! 这一组只测清单本身（类型 / 校验 / 序列化），不启动任何模块——
//! 宿主运行时是后面的事；协议先钉死，宿主照着它实现。

use serde_json::{json, Value};
use yanmo_proto::manifest::{
    ModuleRuntime, ModuleUiForm, UiPlacement, MAX_DESCRIPTION_CHARS, MAX_NAME_CHARS,
};
use yanmo_proto::version::Version;
use yanmo_proto::{ManifestError, ModuleCapabilities, ModuleManifest};

fn manifest_json() -> Value {
    json!({
        "id": "app.example.plot-map",
        "default_language": "zh-Hans",
        "name": { "zh-Hans": "剧情地图", "en": "Plot Map" },
        "description": {
            "zh-Hans": "按章节看人物与地点怎么走。",
            "en": "See how places and people move."
        },
        "author": { "name": "Example Studio", "homepage": "https://example.com" },
        "version": "1.0.0",
        "protocol": 1,
        "min_core_version": "0.106.0",
        "runtime": "wasm",
        "entry": "module.wasm",
        "ui": { "form": "embedded", "placement": "pane" },
        "capabilities": {
            "read": ["works", "tree", "body", "outline"],
            "own_data": true,
            "cross_read": [],
            "ask_questions": false,
            "network": false
        }
    })
}

fn parse(value: &Value) -> Result<ModuleManifest, ManifestError> {
    ModuleManifest::from_json(&value.to_string())
}

fn code(value: &Value) -> &'static str {
    parse(value).unwrap_err().code
}

#[test]
fn minimal_manifest_parses_and_capabilities_default_to_deny() {
    let mut value = manifest_json();
    value["capabilities"] = json!({});
    let manifest = parse(&value).unwrap();
    assert_eq!(manifest.capabilities, ModuleCapabilities::none());
    assert_eq!(manifest.runtime, ModuleRuntime::Wasm);
    assert!(matches!(
        manifest.ui,
        ModuleUiForm::Embedded { placement: UiPlacement::Pane }
    ));
    assert_eq!(manifest.name_text("en"), "Plot Map");
    assert_eq!(manifest.name_text("fr"), "剧情地图", "没有法语就回默认语言");
    assert_eq!(manifest.name_text("zh-Hant"), "剧情地图", "同主语言也算命中");
}

#[test]
fn unknown_fields_are_rejected_instead_of_ignored() {
    let mut value = manifest_json();
    value["certified"] = json!(true);
    assert_eq!(code(&value), ManifestError::JSON);
    let mut value = manifest_json();
    value["capabilities"]["write_core"] = json!(true);
    assert_eq!(code(&value), ManifestError::JSON);
}

#[test]
fn ids_are_reverse_domain_lowercase() {
    for id in ["map", "App.Example.Map", "app..map", ".app.map", "app/map", "app.地图"] {
        let mut value = manifest_json();
        value["id"] = json!(id);
        assert_eq!(code(&value), ManifestError::ID, "id: {id:?}");
    }
}

#[test]
fn localized_text_needs_a_declared_default_and_sane_tags() {
    let mut value = manifest_json();
    value["name"] = json!({ "en": "Plot Map" });
    assert_eq!(code(&value), ManifestError::NAME, "默认语言必须在字典里");

    let mut value = manifest_json();
    value["default_language"] = json!("zh_CN");
    assert_eq!(code(&value), ManifestError::NAME, "语言码要合规");

    let mut value = manifest_json();
    value["description"] = json!({ "zh-Hans": "  " });
    assert_eq!(code(&value), ManifestError::DESCRIPTION);

    let mut value = manifest_json();
    value["name"] = json!({ "zh-Hans": "字".repeat(MAX_NAME_CHARS + 1) });
    assert_eq!(code(&value), ManifestError::NAME);

    let mut value = manifest_json();
    value["description"] = json!({ "zh-Hans": "字".repeat(MAX_DESCRIPTION_CHARS + 1) });
    assert_eq!(code(&value), ManifestError::DESCRIPTION);
}

#[test]
fn protocol_must_be_known_to_this_core() {
    for protocol in [0, 2] {
        let mut value = manifest_json();
        value["protocol"] = json!(protocol);
        assert_eq!(code(&value), ManifestError::PROTOCOL, "protocol: {protocol}");
    }
}

#[test]
fn versions_are_three_numbers() {
    let mut value = manifest_json();
    value["version"] = json!("1.0");
    assert_eq!(code(&value), ManifestError::VERSION);
    let mut value = manifest_json();
    value["min_core_version"] = json!("latest");
    assert_eq!(code(&value), ManifestError::MIN_CORE);
}

#[test]
fn entry_is_a_relative_wasm_inside_the_module_directory() {
    for entry in ["/module.wasm", "\\module.wasm", "../module.wasm", "a/../b.wasm", "module.js", "a//b.wasm"] {
        let mut value = manifest_json();
        value["entry"] = json!(entry);
        assert_eq!(code(&value), ManifestError::ENTRY, "entry: {entry:?}");
    }
    let mut value = manifest_json();
    value["entry"] = json!("dist/module.wasm");
    assert!(parse(&value).is_ok());
}

#[test]
fn all_ui_forms_parse() {
    let mut value = manifest_json();
    value["ui"] = json!({ "form": "standalone" });
    assert_eq!(parse(&value).unwrap().ui, ModuleUiForm::Standalone);

    let mut value = manifest_json();
    value["ui"] = json!({ "form": "headless" });
    assert_eq!(parse(&value).unwrap().ui, ModuleUiForm::Headless);

    let mut value = manifest_json();
    value["ui"] = json!({ "form": "embedded", "placement": "workbench" });
    assert_eq!(
        parse(&value).unwrap().ui,
        ModuleUiForm::Embedded { placement: UiPlacement::Workbench }
    );

    let mut value = manifest_json();
    value["ui"] = json!({ "form": "embedded" });
    assert_eq!(code(&value), ManifestError::JSON, "内嵌必须给落位");
}

#[test]
fn capabilities_reject_duplicates_and_self_cross_read() {
    let mut value = manifest_json();
    value["capabilities"]["read"] = json!(["body", "body"]);
    assert_eq!(code(&value), ManifestError::CAPABILITY);

    let mut value = manifest_json();
    value["capabilities"]["cross_read"] = json!(["app.example.plot-map"]);
    assert_eq!(code(&value), ManifestError::CAPABILITY);

    let mut value = manifest_json();
    value["capabilities"]["cross_read"] = json!(["not a module"]);
    assert_eq!(code(&value), ManifestError::CAPABILITY);

    // 未知的只读区域：JSON 层就拒（稳定码只认 as_str 那一份）
    let mut value = manifest_json();
    value["capabilities"]["read"] = json!(["everything"]);
    assert_eq!(code(&value), ManifestError::JSON);
}

#[test]
fn manifest_round_trips_through_json() {
    let manifest = parse(&manifest_json()).unwrap();
    let text = serde_json::to_string(&manifest).unwrap();
    assert_eq!(ModuleManifest::from_json(&text).unwrap(), manifest);
}

#[test]
fn min_core_version_gates_the_host_core() {
    let manifest = parse(&manifest_json()).unwrap();
    assert!(manifest.accepts_core_version(Version { major: 0, minor: 106, patch: 0 }));
    assert!(manifest.accepts_core_version(Version { major: 1, minor: 0, patch: 0 }));
    assert!(!manifest.accepts_core_version(Version { major: 0, minor: 105, patch: 9 }));
    assert!(manifest.compatible_with_core());

    let mut value = manifest_json();
    value["min_core_version"] = Value::Null;
    assert!(parse(&value).unwrap().accepts_core_version(Version { major: 0, minor: 0, patch: 1 }));
}
