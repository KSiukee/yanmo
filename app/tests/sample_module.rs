//! 随包附带的**示例模块**守卫。
//!
//! 这一份是「别人第一次打开研墨，点两下就能看见模块长什么样」的入口，所以它必须：
//! ① 真在包里（wasm + 清单）；② 真能干点事（声明了只读能力）；③ **不读正文**
//! （体验版的边界：按词表在正文里找候选是完整版的能力）。

use std::fs;
use std::path::Path;

#[test]
fn the_bundled_sample_module_is_present_and_read_only() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/bigmap-lite");
    assert!(
        dir.join("module.wasm").is_file(),
        "示例模块的程序不在包里：{}（重新编一次模块并放回 samples/）",
        dir.display()
    );
    assert!(dir.join("ui/index.html").is_file(), "示例模块缺自带界面");

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).expect("清单读不出来"))
            .expect("清单应当是合法 JSON");

    assert_eq!(manifest["runtime"], "wasm");
    assert_eq!(manifest["ui"]["form"], "embedded");
    let reads: Vec<&str> = manifest["capabilities"]["read"]
        .as_array()
        .expect("能力清单应当是数组")
        .iter()
        .map(|value| value.as_str().unwrap_or_default())
        .collect();
    assert!(reads.contains(&"outline"), "示例至少要能读目录");
    assert!(reads.contains(&"entities"), "示例至少要能读设定卡");
    assert!(
        !reads.contains(&"body"),
        "体验版不许读正文——读正文是完整版（按词表找候选）的能力"
    );
}
