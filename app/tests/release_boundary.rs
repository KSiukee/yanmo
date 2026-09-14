//! 发布边界守卫：**内部工具不流出去、安装包不带工具、出货代码不出网**。
//!
//! 这三条都是"一旦漏了就没法回收"的那类事故：
//! - 内部测试工具（那些不打算公开的那些）比本体更值钱，一旦进了公开仓或安装包，
//!   就等于被迫按 AGPL 暴露；
//! - "研墨不联网"是对外承诺（README / SECURITY 里写死的那条），必须有机械检查背书——
//!   光靠"我们没写"不算证据，出网能力可能被一个依赖顺手带进来。
//!
//! 与 `boundary.rs` 的分工：那边管**界面 ↔ 核心**的接口边界（权限集、命令注册、API 网关），
//! 这边管**主仓 ↔ 内部工具 / 安装包 / 外网**的边界。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn workspace_root() -> PathBuf {
    package_root().parent().expect("工作区根目录").to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()))
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// 去掉注释后的源码（**字符串保留**：要塞进命令行的那种出网口子就藏在字符串里）。
/// 注释里的"将来会做 WebSocket"这种话不算出网能力，不剥掉会天天误报。
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
        } else if ch == '/' && chars.peek() == Some(&'/') {
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            let mut prev = '\0';
            for next in chars.by_ref() {
                if prev == '*' && next == '/' {
                    break;
                }
                prev = next;
            }
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

/// 内部工具的路径特征：出现在**被跟踪的路径**里就是边界漏了。
///
/// ⚠️ 这里刻意**不写任何具体工具仓的名字**——防泄露的守卫自己把名字写进公开文件，
/// 那才是最好笑的漏法。判据只用通用的形状：`*-lab` 这类独立工具仓、以及本仓内部的两处工具目录。
const TOOL_MARKERS: &[&str] = &["tools/qa/", "tools/sanitize/", "-lab/", "/lab/"];

#[test]
fn internal_tools_stay_out_of_the_public_tree() {
    let root = workspace_root();

    // ① .gitignore 必须挡住内部工具（挡住才算"没提交"，删除条目就是敞口）
    let ignore = read(&root.join(".gitignore"));
    for entry in ["tools/qa/", "tools/sanitize/"] {
        assert!(
            ignore.contains(entry),
            ".gitignore 里缺了 `{entry}`——内部工具一旦被跟踪就撤不回来了"
        );
    }

    // ② 拿 git 的真实清单核对：跟踪列表里不许出现工具
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files"])
        .output()
        .expect("这条守卫需要 git（跑不了就是失败，别静默跳过）");
    assert!(output.status.success(), "git ls-files 没跑成：{output:?}");
    let tracked = String::from_utf8_lossy(&output.stdout);
    let leaked: Vec<&str> = tracked
        .lines()
        .filter(|line| TOOL_MARKERS.iter().any(|marker| line.contains(marker)))
        .collect();
    assert!(
        leaked.is_empty(),
        "内部工具进了公开仓（红线）：{leaked:?}\n\
         它们的实现要放独立仓（不链接核心、不进安装包），主仓只留可被驱动的能力"
    );

    // ③ 仓内也不许出现独立工具仓的目录（哪怕还没被跟踪——先出现在树里就该拦住）
    let strays: Vec<String> = fs::read_dir(&root)
        .expect("读工作区根目录")
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with("-lab"))
        .collect();
    assert!(
        strays.is_empty(),
        "仓内出现了独立工具仓的目录：{strays:?}（它该待在仓外，别跟主仓混在一棵树下）"
    );
}

#[test]
fn installer_cannot_ship_internal_tools() {
    let app = package_root();
    let conf: serde_json::Value =
        serde_json::from_str(&read(&app.join("tauri.conf.json"))).expect("配置应当是合法 JSON");

    // ① 打包配置里不许有"把仓库别处的东西一起打进安装包"的口子
    let bundle = conf.get("bundle").expect("bundle 段");
    for key in ["resources", "externalBin", "files"] {
        let value = bundle.get(key);
        assert!(
            value.is_none() || value == Some(&serde_json::Value::Object(Default::default())),
            "bundle.{key} 会把仓库里的别处文件打进安装包（内部工具就是这么漏出去的）：{value:?}"
        );
    }

    // ② 图标这类资源必须待在 app/ 里，不许用 `..` 指向仓库别处
    let icon = bundle
        .get("icon")
        .and_then(|value| value.as_array())
        .expect("bundle.icon 应当是数组");
    assert!(!icon.is_empty(), "图标清单不能空");
    for entry in icon {
        let path = entry.as_str().unwrap_or_default();
        assert!(
            !path.contains(".."),
            "bundle.icon 里有跳出 app/ 的路径：{path}（等于把仓库别处的东西一起打包）"
        );
    }

    // ③ AGPL 分发二进制要**随包给许可证**：安装包配置必须指到仓库根那份 LICENSE
    //    （这一条是合规缺口逼出来的：以前 licenseFile 是空的，装出来的包里没有许可证。）
    let license = bundle.get("licenseFile").and_then(|value| value.as_str()).unwrap_or_default();
    assert_eq!(
        license, "../LICENSE",
        "bundle.licenseFile 应当指向仓库根的 LICENSE（AGPL 分发二进制必须随包给许可证）"
    );
    assert!(
        workspace_root().join("LICENSE").is_file(),
        "仓库根没有 LICENSE：随包的许可证从哪来？"
    );

    // ④ 前端产物与构建命令也只能在 app/ 内（不许拿命令去拽工具目录）
    let frontend = conf
        .get("build")
        .and_then(|build| build.get("frontendDist"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assert!(
        !frontend.contains("..") && !frontend.is_empty(),
        "build.frontendDist 应当指向 app/ 内的目录：{frontend}"
    );
    for key in ["beforeBuildCommand", "beforeDevCommand"] {
        let command = conf
            .get("build")
            .and_then(|build| build.get(key))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        for marker in TOOL_MARKERS {
            assert!(
                !command.contains(marker),
                "{key} 里出现了内部工具路径（{marker}）：{command}"
            );
        }
    }
}

/// 出网能力的特征串。用带引号的形式收"调 curl"这类口子，免得撞上 `curly` 这种词。
const NETWORK_TOKENS: &[&str] = &[
    "TcpListener",
    "TcpStream",
    "UdpSocket",
    "std::net",
    "reqwest",
    "hyper::",
    "ureq",
    "isahc",
    "attohttpc",
    "tokio::net",
    "async_std::net",
    "wstd::",
    "\"curl\"",
    "\"wget\"",
    "Invoke-WebRequest",
    "Invoke-RestMethod",
    "ws2_32",
    "libc::socket",
];

#[test]
fn shipped_code_has_no_network_paths() {
    let root = workspace_root();
    let mut hits = Vec::new();

    // ① 源码（注释剥掉之后再找）
    let mut files = Vec::new();
    for dir in ["crates/yanmo-core/src", "crates/yanmo-cli/src", "crates/yanmo-proto/src", "app/src"] {
        rs_files(&root.join(dir), &mut files);
    }
    // 底线：**累计**扫到的文件太少 = 路径改名了、守卫在空转（按目录判会误伤只有一两个文件的 crate）
    assert!(files.len() >= 40, "只扫到 {} 个源文件，守卫在空转", files.len());
    {
        for file in files {
            let text = without_comments(&read(&file));
            for token in NETWORK_TOKENS {
                if text.contains(token) {
                    hits.push(format!(
                        "{}：出现 `{token}`",
                        file.strip_prefix(&root).unwrap_or(&file).display()
                    ));
                }
            }
        }
    }

    // ② 依赖清单：自身不许引网络库（依赖顺手带进来的出网能力同样算"能出网"）
    let mut manifests = vec![root.join("Cargo.toml")];
    for name in ["yanmo-core", "yanmo-cli", "yanmo-proto"] {
        manifests.push(root.join("crates").join(name).join("Cargo.toml"));
    }
    manifests.push(root.join("app/Cargo.toml"));
    for manifest in manifests {
        let text = read(&manifest);
        for token in NETWORK_TOKENS.iter().filter(|token| !token.starts_with('"')) {
            if text.contains(token) {
                hits.push(format!(
                    "{}：依赖里出现 `{token}`",
                    manifest.strip_prefix(&root).unwrap_or(&manifest).display()
                ));
            }
        }
    }

    assert!(
        hits.is_empty(),
        "出货代码里出现了出网能力（对外承诺是零出网）：\n{}",
        hits.join("\n")
    );
}

/// `Clock` 注入必须**编译期隔离**：发布构建里那段代码根本不存在。
/// 这里只钉住"开关还在、而且是被 feature 关着的"——真正的行为由核心自己的测试证。
#[test]
fn test_clock_injection_stays_behind_a_compile_time_feature() {
    let root = workspace_root();
    let time = without_comments(&read(&root.join("crates/yanmo-core/src/time.rs")));
    assert!(
        time.contains("feature = \"testing\""),
        "Clock 注入点必须用编译期 feature 隔离（发布构建里不许存在）"
    );
    assert!(
        time.contains("YANMO_TEST_CLOCK_MS"),
        "测试时钟的开关变量名变了？改名前先确认测试与文档一起改"
    );
}
