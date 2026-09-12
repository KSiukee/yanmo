//! 边界守卫：把「Rust 持有全部读写、界面不碰文件系统」从口头约定变成**机械检查**。
//!
//! 四条断言，破了任何一条都在 `cargo test` 里红：
//!
//! 1. 核心不依赖任何界面框架（核心必须能被单测 / 命令行直接驱动）；
//! 2. 界面不出现文件系统或取路径的 API；
//! 3. 界面只经 `src/api/` 这一条通道与核心对话；
//! 4. 权限集逐项最小化，不含路径与文件读写能力。
//!
//! 取舍：这里读的是**源文件文本**，不追求完整解析语法。守卫要的是「一眼看懂、破了就拦」，
//! 真正的类型与结构检查留给编译器。

use std::fs;
use std::path::{Path, PathBuf};

/// 界面侧禁止出现的模块——它们要么直接读写文件，要么把真实路径交到界面手里。
const FORBIDDEN_FRONTEND_MODULES: &[&str] = &[
    "@tauri-apps/plugin-fs",
    "@tauri-apps/plugin-dialog",
    "@tauri-apps/plugin-shell",
    "@tauri-apps/plugin-http",
    "@tauri-apps/api/path",
    "node:fs",
    "fs/promises",
    "from \"fs\"",
    "from 'fs'",
    "require(\"fs\")",
];

/// 界面框架——核心一旦依赖它们，就不再能被无界面驱动。
const FORBIDDEN_CORE_DEPS: &[&str] = &["tauri", "tao", "wry", "winit", "webview2"];

/// 网络库——本机数据处理用不上网络；一旦引进来，「零出网」就只剩一句口号。
const FORBIDDEN_NETWORK_DEPS: &[&str] =
    &["reqwest", "hyper", "ureq", "isahc", "curl", "surf", "attohttpc"];

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn workspace_root() -> PathBuf {
    package_root().parent().expect("app 应当在 workspace 根目录下").to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()))
}

/// 递归收集目录下指定后缀的文件（跳过构建产物）。
fn source_files(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", d.display())) {
            let path = entry.expect("目录项可读").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| exts.contains(&e.to_string_lossy().as_ref())) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 取各依赖段落里的依赖名（不含版本号）。
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_deps = matches!(
                line,
                "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
            );
            continue;
        }
        if in_deps && !line.is_empty() && !line.starts_with('#') {
            if let Some((name, _)) = line.split_once('=') {
                names.push(name.trim().to_string());
            }
        }
    }
    names
}

#[test]
fn core_has_no_ui_dependency() {
    let manifest = read(&workspace_root().join("crates/yanmo-core/Cargo.toml"));
    let names = dependency_names(&manifest);
    assert!(!names.is_empty(), "没解析出核心依赖，守卫会假绿——检查 Cargo.toml 结构");
    for name in &names {
        assert!(
            !FORBIDDEN_CORE_DEPS.contains(&name.as_str()),
            "核心不得依赖界面框架「{name}」：它必须能被单元测试与命令行直接驱动"
        );
    }
}

/// 命令行入口的边界：**不开窗、不出网**。
///
/// 它存在的意义就是「没有界面也能把事干完」（自动化、脚本、体检）。一旦引入界面框架，
/// 它就不再是那条轻量的路；一旦引入网络库，「只做本机数据处理」这句承诺也就没法自证了。
#[test]
fn cli_has_no_ui_or_network_dependency() {
    let manifest = read(&workspace_root().join("crates/yanmo-cli/Cargo.toml"));
    let names = dependency_names(&manifest);
    assert!(!names.is_empty(), "没解析出命令行依赖，守卫会假绿——检查 Cargo.toml 结构");
    for name in &names {
        assert!(
            !FORBIDDEN_CORE_DEPS.contains(&name.as_str()),
            "命令行不得依赖界面框架「{name}」：它要在没有窗口的环境里也能跑"
        );
        assert!(
            !FORBIDDEN_NETWORK_DEPS.contains(&name.as_str()),
            "命令行不得依赖网络库「{name}」：这里只做本机数据处理"
        );
    }
}

#[test]
fn frontend_never_touches_the_filesystem() {
    let src = package_root().join("frontend/src");
    let files = source_files(&src, &["ts", "js", "vue"]);
    assert!(!files.is_empty(), "没扫到前端源码，守卫会假绿——检查扫描目录");
    for file in files {
        let content = read(&file);
        for marker in FORBIDDEN_FRONTEND_MODULES {
            assert!(
                !content.contains(marker),
                "{} 出现了「{marker}」——数据位置与文件读写由 Rust 侧负责，界面不得自行接触",
                file.display()
            );
        }
    }
}

#[test]
fn frontend_talks_to_core_only_through_the_gateway() {
    let src = package_root().join("frontend/src");
    let gateway = src.join("api/core.ts");
    assert!(gateway.is_file(), "核心网关应当存在：{}", gateway.display());
    let gateway_source = read(&gateway);
    assert!(
        gateway_source.contains("invoke<") || gateway_source.contains("invoke("),
        "核心网关里没有 invoke——它就不再是唯一通道，这条断言会假绿"
    );

    for file in source_files(&src, &["ts", "js", "vue"]) {
        let content = read(&file);
        if !content.contains("@tauri-apps/api") {
            continue;
        }
        let relative = file
            .strip_prefix(&src)
            .expect("文件在 src 下")
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            relative.starts_with("api/"),
            "只有 src/api/ 下的文件可以直连核心，违规文件：{relative}"
        );
    }
}

#[test]
fn capabilities_grant_no_path_or_file_access() {
    let path = package_root().join("capabilities/default.json");
    let json: serde_json::Value =
        serde_json::from_str(&read(&path)).expect("权限集应当是合法 JSON");
    let permissions: Vec<String> = json["permissions"]
        .as_array()
        .expect("permissions 应当是数组")
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();

    assert!(!permissions.is_empty(), "权限集不能为空");
    for permission in &permissions {
        for forbidden in ["core:path:", "fs:", "path:", "dialog:", "shell:", "http:"] {
            assert!(
                !permission.starts_with(forbidden),
                "权限「{permission}」超出了「界面不碰文件系统」的边界"
            );
        }
        assert!(!permission.contains('*'), "不用通配授权：{permission}");
        assert!(
            permission != "core:default",
            "core:default 会把路径能力一起装回来，必须逐项列举"
        );
    }
}

/// 从源码里读出「声明了哪些命令」（`#[tauri::command]` 之后的 `pub fn`）。
fn declared_commands(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for file in source_files(dir, &["rs"]) {
        let text = read(&file);
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("#[tauri::command") {
                continue;
            }
            for follow in lines.iter().skip(index + 1).take(4) {
                let trimmed = follow.trim();
                if let Some(rest) = trimmed.strip_prefix("pub fn ") {
                    if let Some(name) = rest.split('(').next() {
                        names.push(name.to_string());
                    }
                    break;
                }
            }
        }
    }
    names.sort();
    names
}

/// 从 `main.rs` 的注册清单里读出命令名（`commands::<域>::<名字>`）。
fn registered_commands(main_rs: &str) -> Vec<String> {
    let mut names: Vec<String> = main_rs
        .split("commands::")
        .skip(1)
        .filter_map(|chunk| {
            let path: String = chunk
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            path.split("::").nth(1).map(|name| name.to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[test]
fn every_declared_command_is_registered_and_reachable_from_the_gateway() {
    let commands_dir = package_root().join("src/commands");
    let declared = declared_commands(&commands_dir);
    assert!(!declared.is_empty(), "没扫到命令声明，守卫会假绿——检查扫描目录");

    let registered = registered_commands(&read(&package_root().join("src/main.rs")));
    assert_eq!(
        declared, registered,
        "声明的命令与注册的命令必须一一对应：漏注册 = 界面调不到，注册了不实现 = 启动就崩"
    );

    // 命令都要有界面出口，否则就是没人用的死代码
    let gateway = read(&package_root().join("frontend/src/api/core.ts"));
    for name in &registered {
        assert!(
            gateway.contains(&format!("\"{name}\"")),
            "命令 {name} 没有登记进前端网关（src/api/core.ts 的命令白名单）"
        );
    }
}
