//! 交互菜单的验收：**作者不看文档也能把稿子导出来**。
//!
//! 菜单的输入输出是注入的，所以这里能把"作者会怎么点"直接喂进去跑一遍：
//! 体检、读稿、导出、退出；以及最要紧的一条——**找不到库时绝不建一个空库**。

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use yanmo_cli::args::Args;
use yanmo_cli::execute;
use yanmo_cli::menu;

fn call(dir: &Path, command: &str, options: &[(&str, &str)]) -> serde_json::Value {
    let args = Args {
        data: dir.to_path_buf(),
        command: command.to_string(),
        options: options
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
    };
    let value = execute(&args).unwrap_or_else(|error| panic!("{command} 应当成功：{error}"));
    assert_eq!(value["ok"], true, "{command} 应当成功：{value}");
    value
}

/// 造一本书一章，并写进正文（用开发档命令，最简单的一条路）。
fn seed(dir: &Path) -> (i64, i64) {
    let work = call(dir, "new-work", &[("kind", "novel"), ("title", "长夜")]);
    let work_id = work["work_id"].as_i64().unwrap();
    let node = call(
        dir,
        "new-node",
        &[("work", &work_id.to_string()), ("kind", "chapter"), ("title", "第一章")],
    );
    let node_id = node["node_id"].as_i64().unwrap();
    call(dir, "write", &[("node", &node_id.to_string()), ("body", "第一段。\n\n第二段。")]);
    (work_id, node_id)
}

/// 跑一遍菜单，返回它打印出来的全部文本。
fn drive(dir: &Path, export_root: &Path, keys: &str) -> String {
    let argv = vec!["--data".to_string(), dir.to_string_lossy().to_string()];
    let mut out: Vec<u8> = Vec::new();
    let mut input = Cursor::new(keys.as_bytes().to_vec());
    menu::run_with_root(&argv, Some(export_root), &mut out, &mut input)
        .unwrap_or_else(|error| panic!("菜单应当正常跑完：{error}"));
    String::from_utf8(out).expect("输出应当是 UTF-8")
}

#[test]
fn health_check_speaks_plain_chinese() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());

    let text = drive(dir.path(), exports.path(), "2\n5\n");

    assert!(text.contains("备用导出工具"), "{text}");
    assert!(text.contains("长夜"), "开屏要报出书架上有什么：{text}");
    assert!(text.contains("库文件完整性"), "{text}");
    assert!(text.contains("看起来没问题"), "体检要给出人话结论：{text}");
    assert!(text.contains("再见"), "选 5 要能退出：{text}");
}

#[test]
fn reading_a_chapter_shows_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());

    // 1 = 读取稿子 → 选第 1 本 → 选第 1 章 → 5 = 退出
    let text = drive(dir.path(), exports.path(), "1\n1\n1\n5\n");

    assert!(text.contains("要读哪一本"), "{text}");
    assert!(text.contains("第一段。"), "正文要看得见：{text}");
    assert!(text.contains("第二段。"), "{text}");
}

#[test]
fn exporting_one_book_writes_files_and_says_where() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());

    // 3 = 导出某一本 → 第 1 本 → 两种格式 → 退出
    let text = drive(dir.path(), exports.path(), "3\n1\n1\n5\n");

    assert!(text.contains("✅"), "导出要给个明确回执：{text}");
    assert!(text.contains("长夜"), "{text}");
    let written = std::fs::read_dir(exports.path()).unwrap().count();
    assert!(written >= 1, "导出目录里应当真有东西：{}", exports.path().display());
}

#[test]
fn exporting_everything_covers_all_books() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());
    call(dir.path(), "new-work", &[("kind", "article"), ("title", "短歌")]);

    let text = drive(dir.path(), exports.path(), "4\n5\n");

    assert!(text.contains("长夜") && text.contains("短歌"), "两本都要导：{text}");
    assert!(text.contains("合计导出"), "要给一句汇总：{text}");
}

#[test]
fn wrong_choice_does_not_crash_or_exit() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());

    let text = drive(dir.path(), exports.path(), "9\n奇怪的东西\n5\n");

    assert!(text.contains("不是 1-5 里的选项"), "{text}");
    assert!(text.contains("再见"), "乱输之后仍然能正常退出：{text}");
}

#[test]
fn a_directory_without_a_database_is_never_silently_created() {
    let empty = tempfile::tempdir().unwrap();
    let argv = vec!["--data".to_string(), empty.path().to_string_lossy().to_string()];
    let mut out: Vec<u8> = Vec::new();
    let mut input = Cursor::new(Vec::new());

    let error = menu::run_with(&argv, &mut out, &mut input).unwrap_err();

    assert!(error.contains("没有找到稿子库"), "要把话说明白：{error}");
    assert!(
        !empty.path().join("yanmo.db").exists(),
        "绝不能建一个空库让作者以为稿子丢了"
    );
}

#[test]
fn an_existing_library_is_used_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());

    // 只给了 --data，没有任何交互输入：菜单跑完（读到输入结束就退出）
    let text = drive(dir.path(), exports.path(), "");

    assert!(text.contains("数据目录"), "{text}");
    assert!(!text.contains("请粘贴数据目录"), "库在就不该再问作者：{text}");
}

/// `--data` 的取值不会被当成"命令"（这条错了，菜单就永远打不开）。
#[test]
fn menu_mode_is_detected_even_with_data_only() {
    let dir = tempfile::tempdir().unwrap();
    let exports = tempfile::tempdir().unwrap();
    seed(dir.path());
    let _ = PathBuf::from(dir.path());

    let text = drive(dir.path(), exports.path(), "5\n");
    assert!(text.contains("请选择"), "只给 --data 时应当进菜单：{text}");
}
