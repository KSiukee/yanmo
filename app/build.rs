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
//! 2. 文件不在、但 CLI 已经编过 → 直接搬过去（本地开发最常见的路径，自动愈合）；
//! 3. 两边都没有 → 打一句人话，告诉作者该跑哪条命令，而不是让 Tauri 抛原始错误。
//!
//! 与 `tools/cli_package.py` 是同一件事的两半：那边是**权威**（出包时现编现放），
//! 这边只是给本地开发者兜底。名字/路径两边必须一致：`app/binaries/yanmo-cli-<三元组>.exe`。

use std::path::PathBuf;

fn main() {
    if let Err(message) = stage_sidecar() {
        panic!("{message}");
    }
    tauri_build::build()
}

/// 把 `externalBin` 要的文件放到 `app/binaries/yanmo-cli-<三元组>.exe`。
fn stage_sidecar() -> Result<(), String> {
    let app = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let root = app
        .parent()
        .ok_or_else(|| "app 目录没有上级目录（仓库结构变了？）".to_string())?
        .to_path_buf();
    let triple = std::env::var("TARGET").map_err(|e| e.to_string())?;
    let staged = app.join("binaries").join(format!("yanmo-cli-{triple}.exe"));
    if staged.is_file() {
        return Ok(());
    }

    // 已经编过就搬过去：release 优先，debug 也收（本地反复调试时省一次 release 编译）
    let built = [
        root.join("target").join("release").join("yanmo-cli.exe"),
        root.join("target").join("debug").join("yanmo-cli.exe"),
    ];
    let Some(source) = built.iter().find(|path| path.is_file()) else {
        return Err(format!(
            "随包的命令行工具（yanmo-cli）还没编出来——安装包必须带上它：\n\
             界面打不开（显卡 / 驱动 / 白屏）或者库坏掉时，它是唯一的取稿路。\n\
             \n\
             先在仓库根跑一次：\n    cargo build --release -p yanmo-cli\n\
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
            "把 CLI 放到随包位置失败（{} → {}）：{error}",
            source.display(),
            staged.display()
        )
    })?;
    Ok(())
}
