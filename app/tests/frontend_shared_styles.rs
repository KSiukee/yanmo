//! 共享样式表的守卫：**用了别人的类名，就得把那份样式表引进来**。
//!
//! 为什么要有这一条：Vue 的 `scoped` 样式只作用于**本组件模板里的元素**。
//! 组件用了 `dialog__button` / `settings__row` 这类共享类名、却没有
//! `<style scoped src="./xxx.css">` 时，编译器不报错、页面也不空白——
//! 它只是悄悄掉回浏览器默认样式。这个坑踩过三次，三次都是人眼发现的：
//!   ① 模块页表头（图标 SVG 没尺寸，撑成一张铺满卡片的大图）
//!   ② 创作流「记成」的系统下拉框
//!   ③ 设置页「叩问」整页 16px
//!
//! 为什么写在这边（Rust 测试）而不是前端测试：前端源码**不许碰文件系统**
//! （`boundary.rs` 有一条守卫专门盯着 `node:fs`），而这条检查要读一堆 .vue 文件。
//! 放在 Rust 测试里，既读得到，也不破坏「界面不碰文件」这条边界。

use std::fs;
use std::path::{Path, PathBuf};

/// 共享样式表 → 它负责的类名记号。
const SHARED: &[(&str, &[&str])] = &[
    ("dialog.css", &["dialog__"]),
    ("settings-dialog.css", &["settings__"]),
    (
        "module-toolbar.css",
        &[
            "modules__head",
            "modules__heading",
            "modules__title",
            "modules__icon",
            "modules__sub",
            "modules__tools",
            "modules__close",
        ],
    ),
    (
        "module-dialog.css",
        &[
            "modules__box",
            "modules__list",
            "modules__card",
            "modules__empty",
            "modules__sample",
            "modules__row",
            "modules__name",
        ],
    ),
];

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 只取模板段：`<script>` 里的字符串、`<style>` 里的定义都不算「用了这个类」。
fn template_of(text: &str) -> &str {
    let start = match text.find("<template>") {
        Some(index) => index + "<template>".len(),
        None => return "",
    };
    let end = text.find("</template>").unwrap_or(text.len());
    &text[start..end]
}

fn vue_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().map(|ext| ext == "vue").unwrap_or(false))
            .collect(),
        Err(_) => Vec::new(),
    };
    // 主布局也一起看
    files.push(dir.join("../../App.vue"));
    files.sort();
    files
}

#[test]
fn shared_classes_require_their_stylesheet() {
    let dir = package_root().join("frontend/src/components");
    let files = vue_files(&dir);
    assert!(files.len() > 20, "没扫到组件，守卫会假绿——检查目录：{}", dir.display());

    let mut violations: Vec<String> = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let template = template_of(&text);
        if template.is_empty() {
            continue;
        }
        for (style, markers) in SHARED {
            if !markers.iter().any(|marker| template.contains(marker)) {
                continue;
            }
            let needle = format!("src=\"./{style}\"");
            if !text.contains(&needle) {
                violations.push(format!(
                    "{} 用了共享类名，但没引 {style}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "scoped 样式不会作用到没引它的组件里，这些地方的控件会掉回系统默认样式：\n{}",
        violations.join("\n")
    );
}
