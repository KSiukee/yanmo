//! 边界守卫：把「Rust 持有全部读写、界面不碰文件系统」从口头约定变成**机械检查**。
//!
//! 五条断言，破了任何一条都在 `cargo test` 里红：
//!
//! 1. 核心不依赖任何界面框架（核心必须能被单测 / 命令行直接驱动）；
//! 2. 界面不出现文件系统或取路径的 API；
//! 3. 界面只经 `src/api/` 这一条通道与核心对话；
//! 4. 权限集逐项最小化，不含路径与文件读写能力；
//! 5. 三栏骨架的布局不变量：正文再长，也只有正文区自己滚。
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

/// 取样式文件里的某条规则（`选择器 {` 到第一个 `}`）。
fn rule_block(text: &str, selector: &str) -> String {
    let start = text
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("找不到 {selector} 的规则块"));
    let end = text[start..].find('}').map(|at| start + at).expect("规则块没闭合");
    text[start..end].to_string()
}

/// 取样式文件里的某条规则（`选择器 {` 到第一个 `}`）——**没有这条规则就是 `None`**。
///
/// 组合选择器（`.a,\n.b {`）不算命中：找的是带空格的 `.a {`，所以"这一格自己有没有一条规则"
/// 不会被邻居的规则块混进来。
fn rule_block_optional(text: &str, selector: &str) -> Option<String> {
    let needle = format!("{selector} {{");
    let start = text.find(&needle)?;
    let end = text[start..].find('}').map(|at| start + at)?;
    Some(text[start..end].to_string())
}

/// 三栏骨架的布局不变量：**正文再长，也只有正文区自己滚**。
///
/// 真踩过：长章节把 CSS 网格的隐含行顶高（网格项默认 `min-height: auto`），整个页面跟着滚，
/// 顶栏与两侧栏一起飘出视野。这类毛病**短稿永远测不出来**（只有内容够长才犯），正适合机器盯着。
#[test]
fn the_shell_keeps_the_page_still_when_the_prose_gets_long() {
    let app = read(&package_root().join("frontend/src/App.vue"));
    let shell = rule_block(&app, ".shell__body");
    assert!(
        shell.contains("grid-template-rows: minmax(0, 1fr)"),
        "三栏容器必须钉住行高，否则长正文会把整页顶开：{shell}"
    );
    assert!(shell.contains("overflow: hidden"), "三栏容器还要裁住溢出：{shell}");

    let css = read(&package_root().join("frontend/src/components/editor-pane.css"));
    let editor = rule_block(&css, ".editor");
    assert!(
        editor.contains("min-height: 0"),
        "编辑栏是网格项，必须显式 min-height: 0（默认 auto 会被内容顶高）：{editor}"
    );
    let area = rule_block(&css, ".editor__area");
    assert!(area.contains("overflow: auto"), "正文区自己滚：{area}");

    // 界面目前只有亮色一套：color-scheme 写 `light dark` 会让系统深色偏好把**默认滚动条**
    // 画成深色（浅色稿纸配黑滚动条，真报过）。等暗色模式那条任务落地时，这条要跟着改。
    let base = read(&package_root().join("frontend/src/style.css"));
    let root = rule_block(&base, ":root");
    // 盯的是**声明**本身，不是注释里出现的字样（注释里解释这件事时会提到那个写法）
    assert!(
        root.contains("color-scheme: light;") && !root.contains("color-scheme: light dark"),
        "界面只有亮色一套：:root 的 color-scheme 要写 light（别写 light dark）：{root}"
    );
}

/// 状态栏右边那一组：**只钉住"爱变的那一格"**，其余按内容自然排。
///
/// 两件真报过的事，正好是一对矛盾，这条守卫把两边一起钉住：
///
/// 1. 起伏：保存状态在「待落盘…」（4 字位）与「已保存」（3 字位）之间来回切，整组宽度跟着变，
///    前面那排按钮（版本/排版/一句话）就被推着小幅左右晃——正写字的人被晃得没法专注；
/// 2. 空白：若给**每一格**都钉宽度，多出来的宽度会落在格与格之间，看着"隔开太多"、没以前顺眼。
///
/// 所以规矩是：只有保存状态那格 `min-width` 钉死，且内容**左对齐**（多出的宽度落在右缘，
/// 看不见）；字数 / 语言 / 今日一律**不许钉宽**，格与格之间恒是那 10px。
/// 这类毛病只有真机连续打字才看得出，短稿与静态截图都测不出来，所以让机器盯着。
#[test]
fn only_the_flipping_status_slot_is_width_pinned() {
    let css = read(&package_root().join("frontend/src/components/editor-pane.css"));

    // ① 保存状态那格：能拿宽度的盒子 + 固定宽度 + 左对齐（多出的宽度留在右缘）
    let status = rule_block(&css, ".editor__status");
    for needed in ["display: inline-block", "min-width:", "text-align: left"] {
        assert!(
            status.contains(needed),
            "保存状态那格必须固定宽度且内容左对齐（缺 `{needed}`）——它是唯一频繁变长变短的格子：{status}"
        );
    }

    // ② 其余三格**不许**钉宽度：钉了就会在格与格之间撑出一排空白（"隔开太多"就是这么来的）
    for selector in [".editor__count", ".editor__lang", ".editor__today"] {
        if let Some(block) = rule_block_optional(&css, selector) {
            assert!(
                !block.contains("min-width"),
                "{selector} 不该钉宽度（会撑出空白，让状态栏看着稀疏）；只钉保存状态那一格：{block}"
            );
        }
    }

    // ③ 共用的那几条：不参与收缩（flex: none）、不换行
    let shared = rule_block(&css, ".editor__count,\n.editor__lang,\n.editor__today,\n.editor__status");
    for needed in ["flex: none", "white-space: nowrap"] {
        assert!(shared.contains(needed), "状态栏各格共用规则缺 `{needed}`：{shared}");
    }

    // ④ 达标后的短记号（进度条让位给它）必须**沿用**进度条那 36px 的槽位——自己另定宽度的话，
    //    作者写到目标那一刻整条栏就会重排。满格的进度条没有信息量，但"去掉它"不等于"改槽位"。
    let done = rule_block(&css, ".editor__today-bar--done");
    assert!(
        !done.contains("width:"),
        "达标记号不该自己另定宽度（要沿用进度条的槽位，否则达标瞬间整条栏会重排）：{done}"
    );
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
