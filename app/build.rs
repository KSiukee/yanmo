//! 壳的构建脚本：先把随包资源备齐，再交给 Tauri。
//!
//! # 为什么多这一层（2026-09-20）
//!
//! 安装包必须**随包带命令行工具** `yanmo-cli.exe`——界面打不开时它是唯一的取稿路
//! （README 与 SECURITY 都这么承诺）。做法是 Tauri 的 `bundle.externalBin`，而 Tauri
//! 在**编译期**就要求那个资源存在（`binaries/yanmo-cli-<三元组>.exe`）。于是"新克隆仓库
//! 直接 `cargo test`"会断在一句很不好懂的 `resource path ... doesn't exist` 上。
//!
//! 这里把那件事接住：
//! 1. 文件已就位 → 什么都不做（出包脚本 `tools/cli_package.py` 放的就是它）；
//! 2. 文件不在、但已经编过 → 直接搬过去（本地开发最常见的路径，自动愈合）；
//! 3. 两边都没有 → 打一句人话，告诉作者该跑哪条命令，而不是让 Tauri 抛原始错误。
//!
//! # 模块平台（0.120.0）
//!
//! 正式出包时还要带 `yanmo-host.exe` / `yanmo-launch.exe`（管理页「启动」要用）。
//! 发布脚本先编好它们，再设 `YANMO_MODULE_PLATFORM=1` 调 `cargo tauri build`；
//! 本脚本看到这个变量就把两个二进制也搬进 `app/binaries/`，由
//! `tauri.release.conf.json` 的 `externalBin` 收进安装包。普通 `cargo test` 不要求它们，
//! 所以新克隆仓库不会被模块平台卡住。

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=YANMO_MODULE_PLATFORM");
    if let Err(message) = stage_sidecar("yanmo-cli", true) {
        panic!("{message}");
    }
    if std::env::var("YANMO_MODULE_PLATFORM").ok().as_deref() == Some("1") {
        for name in ["yanmo-host", "yanmo-launch"] {
            if let Err(message) = stage_sidecar(name, true) {
                panic!("{message}");
            }
        }
    }
    tauri_build::build()
}

/// 把 `externalBin` 要的文件放到 `app/binaries/<name>-<三元组>.exe`。
fn stage_sidecar(name: &str, required: bool) -> Result<(), String> {
    let app = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let root = app
        .parent()
        .ok_or_else(|| "app 目录没有上级目录（仓库结构变了？）".to_string())?
        .to_path_buf();
    let triple = std::env::var("TARGET").map_err(|e| e.to_string())?;
    let staged = app.join("binaries").join(format!("{name}-{triple}.exe"));
    if staged.is_file() {
        return Ok(());
    }

    // 已经编过就搬过去：release 优先，debug 也收（本地反复调试时省一次 release 编译）
    let built = [
        root.join("target").join("release").join(format!("{name}.exe")),
        root.join("target").join("debug").join(format!("{name}.exe")),
    ];
    let Some(source) = built.iter().find(|path| path.is_file()) else {
        if !required {
            return Ok(());
        }
        let how = if name == "yanmo-cli" {
            "cargo build --release -p yanmo-cli"
        } else {
            "cargo build --release -p yanmo-host -p yanmo-launcher"
        };
        return Err(format!(
            "随包的 {name} 还没编出来——安装包必须带上它：\n\
             \n\
             先在仓库根跑一次：\n    {how}\n\
             然后重新构建即可。\n\
             \n\
             （出包的 `tools\\build-release.bat` 会自动做这一步并放到位；\n\
             　这一步的说明也在 CONTRIBUTING.md 的「开发须知」里。）"
        ));
    };

    if let Some(parent) = staged.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("建不了 {}：{error}", parent.display()))?;
    }
    std::fs::copy(source, &staged).map_err(|error| {
        format!(
            "把 {name} 放到随包位置失败（{} → {}）：{error}",
            source.display(),
            staged.display()
        )
    })?;
    Ok(())
}
