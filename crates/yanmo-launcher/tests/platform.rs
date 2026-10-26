//! 平台模式：一个 launcher 起一个多模块宿主，`run <id>` 分两次跑。

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::TempDir;
use yanmo_core::store::Store;
use yanmo_proto::capability::{CoreDataArea, ModuleCapabilities};
use yanmo_proto::manifest::ModuleManifest;

fn host_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("YANMO_HOST_BIN") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let name = if cfg!(windows) { "yanmo-host.exe" } else { "yanmo-host" };
    let path = dir.join(name);
    path.is_file().then_some(path)
}

fn package(root: &Path, id: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": id,
            "default_language": "zh-Hans",
            "name": { "zh-Hans": id },
            "description": { "zh-Hans": "平台测试。" },
            "author": { "name": "Example Studio" },
            "version": "1.0.0",
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": [] }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("module.wasm"),
        wat::parse_str(r#"(module (func (export "run") (result i32) (i32.const 0)))"#).unwrap(),
    )
    .unwrap();
}

#[test]
fn platform_launcher_runs_two_modules() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    package(dir.path(), "app.example.platform.one");
    package(dir.path(), "app.example.platform.two");
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "30",
            "--idle-secs",
            "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    for id in ["app.example.platform.one", "app.example.platform.two"] {
        let mut line = String::new();
        stdin.write_all(format!("run {id}\n").as_bytes()).unwrap();
        stdin.flush().unwrap();
        reader.read_line(&mut line).unwrap();
        let report: Value = serde_json::from_str(line.trim()).expect("每次运行一行报告");
        assert_eq!(report["module_id"], id);
        assert_eq!(report["status"]["kind"], "ok");
    }

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
}

fn read_package(root: &Path, id: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": id,
            "default_language": "zh-Hans",
            "name": { "zh-Hans": id },
            "description": { "zh-Hans": "模块身份路由测试。" },
            "author": { "name": "Example Studio" },
            "version": "1.0.0",
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": ["works"] }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("module.wasm"),
        wat::parse_str(r#"
            (module
              (import "env" "read" (func $read (param i32 i32 i32 i32 i32) (result i32)))
              (import "env" "emit" (func $emit (param i32 i32) (result i32)))
              (memory (export "memory") 1)
              (func (export "run") (result i32)
                (local $n i32)
                (local.set $n
                  (call $read (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 100) (i32.const 4096)))
                (if (i32.lt_s (local.get $n) (i32.const 0))
                  (then (return (local.get $n))))
                (call $emit (i32.const 100) (local.get $n))))
        "#).unwrap(),
    )
    .unwrap();
}

#[test]
fn platform_launcher_routes_module_identity() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    let id = "app.example.platform.read";
    read_package(dir.path(), id);
    let manifest_text = fs::read_to_string(dir.path().join(id).join("manifest.json")).unwrap();
    let manifest = ModuleManifest::from_json(&manifest_text).unwrap();
    let grant = ModuleCapabilities {
        read: vec![CoreDataArea::Works],
        ..ModuleCapabilities::none()
    };
    let db = dir.path().join("yanmo.db");
    let mut store = Store::open(&db).unwrap();
    store.install_module(&manifest, &grant).unwrap();
    drop(store);

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "30",
            "--idle-secs",
            "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(format!("run {id}\n").as_bytes()).unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let report: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(report["module_id"], id);
    assert_eq!(report["status"]["kind"], "ok", "{report}");

    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn platform_launcher_stops_when_parent_stdin_closes() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    package(dir.path(), "app.example.longrun");
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "60",
            "--idle-secs",
            "60",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(b"run app.example.longrun\n")
        .unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(120));
    drop(stdin);

    let started = std::time::Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "壳 stdin 关闭后平台应在 5 秒内收掉宿主"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// 断链：作者掐断的边，唤起**一行都不转发**，但要有终局（`launch.chain_cut`）和一行 `cut` 留痕。
#[test]
fn a_cut_edge_is_refused_with_a_terminal_state() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    package(dir.path(), "app.example.chain.a");
    package(dir.path(), "app.example.chain.b");
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();
    {
        use yanmo_core::store::ModuleData;
        let mut data = ModuleData::open(ModuleData::path_beside(&db)).unwrap();
        data.cut_edge("app.example.chain.a", "app.example.chain.b").unwrap();
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "--modules-root",
            dir.path().to_str().unwrap(),
            "--host",
            host_bin.to_str().unwrap(),
            "--timeout-secs",
            "30",
            "--idle-secs",
            "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    // 被断链：宿主**不该**收到这条 run（所以不会有一行 ok 报告），但仍要有终局
    stdin
        .write_all(b"run app.example.chain.b from:app.example.chain.a\n")
        .unwrap();
    stdin.flush().unwrap();
    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());

    let mut lines = String::new();
    use std::io::Read;
    reader.read_to_string(&mut lines).unwrap();
    let final_report: Value = lines
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|value| value.get("modules").is_some())
        .expect("收工时要有整份报告");
    let modules = final_report["modules"].as_array().unwrap();
    let b = modules
        .iter()
        .find(|item| item["module_id"] == "app.example.chain.b")
        .expect("被拒的唤起也要有终局条目");
    assert_eq!(b["ok"], false);
    assert_eq!(b["code"], "launch.chain_cut");

    let data = yanmo_core::store::ModuleData::open(yanmo_core::store::ModuleData::path_beside(&db)).unwrap();
    let runs = data.recent_runs("app.example.chain.b", 5).unwrap();
    assert_eq!(runs.len(), 1, "被拒也要留一行");
    assert_eq!(runs[0].trigger, "chain:app.example.chain.a");
    assert_eq!(runs[0].outcome, "cut");
}

/// 模块间唤起的最小 ABI：A 在报告里 `emit {"wake":"B"}` → 启动器替它叫 B，
/// B 的留痕来源是 `chain:A`；被断掉的边一次都不转发。
#[test]
fn a_module_can_wake_another() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    // A 会请求唤起 B；B 什么都不做
    let dir = TempDir::new().unwrap();
    package_waker(dir.path(), "app.example.wake.a", "app.example.wake.b");
    package(dir.path(), "app.example.wake.b");
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db", db.to_str().unwrap(),
            "--modules-root", dir.path().to_str().unwrap(),
            "--host", host_bin.to_str().unwrap(),
            "--timeout-secs", "30",
            "--idle-secs", "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    stdin.write_all(b"run app.example.wake.a\n").unwrap();
    stdin.flush().unwrap();
    // 等 B 的报告（它由 A 的唤醒触发，不是我们发的）
    let mut seen_b = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if line.contains("app.example.wake.b") && line.contains("\"ok\":true") {
            seen_b = true;
            break;
        }
    }
    stdin.write_all(b"stop\n").unwrap();
    stdin.flush().unwrap();
    assert!(child.wait().unwrap().success());
    assert!(seen_b, "A 的唤醒应当真把 B 叫起来，并有一份 ok 报告");

    let data = yanmo_core::store::ModuleData::open(yanmo_core::store::ModuleData::path_beside(&db)).unwrap();
    let runs = data.recent_runs("app.example.wake.b", 5).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].trigger, "chain:app.example.wake.a", "来源要记得是谁叫的");
}

/// 一个"会叫别人"的模块：报告里 emit 一条 `{"wake":...}`。
fn package_waker(root: &Path, id: &str, target: &str) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::json!({
            "id": id,
            "default_language": "zh-Hans",
            "name": { "zh-Hans": id },
            "description": { "zh-Hans": "唤起测试。" },
            "author": { "name": "Example Studio" },
            "version": "1.0.0",
            "protocol": 1,
            "runtime": "wasm",
            "entry": "module.wasm",
            "ui": { "form": "headless" },
            "capabilities": { "read": [] }
        })
        .to_string(),
    )
    .unwrap();
    let payload = format!("{{\"wake\":\"{target}\"}}");
    let escaped = payload.replace('"', "\\\"");
    let wat = format!(
        r#"(module
          (import "env" "emit" (func $emit (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "{escaped}")
          (func (export "run") (result i32)
            (drop (call $emit (i32.const 0) (i32.const {len})))
            (i32.const 0)))"#,
        len = payload.len()
    );
    fs::write(dir.join("module.wasm"), wat::parse_str(&wat).unwrap()).unwrap();
}

/// 断掉的边：A 照样跑，但它叫不动 B（B 一行报告都不该出现）。
#[test]
fn a_cut_edge_stops_a_wake() {
    let Some(host_bin) = host_bin() else {
        eprintln!("skip: yanmo-host binary not built (run `cargo build -p yanmo-host`)");
        return;
    };
    let dir = TempDir::new().unwrap();
    package_waker(dir.path(), "app.example.cut.a", "app.example.cut.b");
    package(dir.path(), "app.example.cut.b");
    let db = dir.path().join("yanmo.db");
    Store::open(&db).unwrap();
    {
        use yanmo_core::store::ModuleData;
        let mut data = ModuleData::open(ModuleData::path_beside(&db)).unwrap();
        data.cut_edge("app.example.cut.a", "app.example.cut.b").unwrap();
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_yanmo-launch"))
        .args([
            "--db", db.to_str().unwrap(),
            "--modules-root", dir.path().to_str().unwrap(),
            "--host", host_bin.to_str().unwrap(),
            "--timeout-secs", "30",
            "--idle-secs", "30",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(b"run app.example.cut.a\n").unwrap();
    stdin.flush().unwrap();
    // 给 A 足够时间跑完并"尝试"叫 B；然后关 stdin 让平台收摊
    std::thread::sleep(std::time::Duration::from_secs(3));
    drop(stdin);
    let mut output = String::new();
    use std::io::Read;
    let _ = reader.read_to_string(&mut output);
    let _ = child.wait().unwrap();
    // 注意：A 的报告里带着 `{"wake":"…cut.b"}`，所以只看"B 自己的报告行"
    assert!(
        !output.contains("\"module_id\":\"app.example.cut.b\""),
        "断掉的边不许把 B 叫起来"
    );
    let data = yanmo_core::store::ModuleData::open(yanmo_core::store::ModuleData::path_beside(&db)).unwrap();
    assert!(data.recent_runs("app.example.cut.b", 5).unwrap().is_empty(), "B 一次都没跑过");
}
