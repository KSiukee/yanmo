//! 坏库打捞命令的验收：默认只看、`--yes` 才写、目标已存在就拒绝。

use std::path::Path;

use yanmo_cli::args::Args;
use yanmo_cli::{execute, CliError};
use yanmo_core::model::WorkKind;
use yanmo_core::store::Store;

fn args(dir: &Path, options: &[(&str, &str)]) -> Args {
    Args {
        data: dir.to_path_buf(),
        command: "salvage".to_string(),
        options: options.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
}

fn seeded(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("yanmo.db");
    let mut store = Store::open(&source).unwrap();
    store.create_work(WorkKind::Novel, "长夜").unwrap();
    source
}

#[test]
fn dry_run_does_not_write_and_yes_produces_a_readable_database() {
    let dir = tempfile::tempdir().unwrap();
    let source = seeded(dir.path());
    let target = dir.path().join("recovered.db");

    let plan = execute(&args(
        dir.path(),
        &[("from", &source.to_string_lossy()), ("to", &target.to_string_lossy())],
    ))
    .unwrap();
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["plan"]["looks_like_sqlite"], true);
    assert!(!target.exists(), "不给 --yes 不许写目标");

    let done = execute(&args(
        dir.path(),
        &[
            ("from", &source.to_string_lossy()),
            ("to", &target.to_string_lossy()),
            ("yes", "true"),
        ],
    ))
    .unwrap();
    assert_eq!(done["dry_run"], false);
    assert!(target.is_file());
    assert_eq!(done["report"]["integrity"][0], "ok");
    assert_eq!(Store::open(&target).unwrap().list_works().unwrap()[0].title, "长夜");
}

#[test]
fn refuses_an_existing_target_and_the_same_path() {
    let dir = tempfile::tempdir().unwrap();
    let source = seeded(dir.path());
    let target = dir.path().join("already.db");
    std::fs::write(&target, b"occupied").unwrap();

    let error = execute(&args(
        dir.path(),
        &[("from", &source.to_string_lossy()), ("to", &target.to_string_lossy()), ("yes", "true")],
    ))
    .unwrap_err();
    match error {
        CliError::Refused { code, .. } => assert_eq!(code, "salvage.target_exists"),
        other => panic!("应当是拒绝执行：{other:?}"),
    }
    assert_eq!(std::fs::read(&target).unwrap(), b"occupied", "拒绝时目标原样");

    let error = execute(&args(
        dir.path(),
        &[("from", &source.to_string_lossy()), ("to", &source.to_string_lossy()), ("yes", "true")],
    ))
    .unwrap_err();
    match error {
        CliError::Refused { code, .. } => assert_eq!(code, "salvage.same_path"),
        other => panic!("应当是拒绝执行：{other:?}"),
    }
}
