// i18n-allow-file: 本模块给出的中文是**验收报告里的拒绝理由**（写进报告文件，
// 与 `acceptance.rs` 同性质），不是界面文案；界面文案仍然只在 `frontend/src/locales/`。
//! 验收沙箱的守卫：**清空一个目录之前，先证明它是我们自己的**。
//!
//! 为什么单独一个文件：这段逻辑与"验收怎么跑"无关，它只回答一个问题——"这个目录能不能删"。
//! 2026-09-15 代码质量评审的严重 7 就是它缺位造成的：`--dir` 参数一路没有校验，
//! 一个参数写错就 `remove_dir_all` 掉整个稿库（不可恢复）。
//! 拆出来也让 `acceptance.rs` 别再往上帝文件那边涨（这次拆分正是按它的登记理由做的）。

use std::path::Path;

/// 沙箱记号：验收模式在自己造的数据目录里留一个文件。
/// **清空一个目录之前必须先看到它**——没有这个记号、又不在系统临时目录里的目录，不该由我们删。
pub(crate) const SCRATCH_MARKER: &str = ".yanmo-acceptance-scratch";

/// 清空数据目录之前，必须先证明「这是我们自己的沙箱」。
///
/// 为什么这道门必须有：`run_bench` 第一件事就是把数据目录清空重来（上一次的数字不该混进这一次），
/// 而数据目录由 `--dir` 参数给。参数打错一个字、或者从批处理里透传进来一个路径，
/// 就会把作者的真稿库连同里面的备份包一起 `remove_dir_all` 掉——**不可恢复**。
/// 所以放行的只有两种目录：
///
/// ① 系统临时目录之下的**空目录**（自定义落点第一次跑时就是这种：还没东西可删）；
/// ② 带沙箱记号文件的（验收模式自己造过、并留了记号的目录）。
///
/// 另外，**只要里面有稿库又没记号，一律拒绝**——哪怕它落在临时目录里。
/// 唯一的例外是那个默认沙箱路径（见 [`is_our_scratch`]）。
/// 拒绝时一个字节都不动，并且把原因写成报告里的第一步。
pub(crate) fn wipe_guard(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(()); // 还没有这个目录：下面会建，没有东西可删
    }
    let real = dir
        .canonicalize()
        .map_err(|error| format!("路径读不出来（{}）：{error}", dir.display()))?;
    if real.parent().is_none() {
        return Err(format!("拒绝清理 {}：那是盘根目录。", real.display()));
    }
    let temp = std::env::temp_dir();
    let temp = temp.canonicalize().unwrap_or(temp);
    let ours = is_our_scratch(&real, &temp, real.join(SCRATCH_MARKER).is_file());
    if !ours && !real.starts_with(&temp) {
        return Err(format!(
            "拒绝清理 {}：它既不在系统临时目录下，也没有验收沙箱的记号（{SCRATCH_MARKER}）。\
             验收模式只清自己造的目录——换一个空目录，或者直接用默认的临时目录。",
            real.display()
        ));
    }
    if !ours && real.join(yanmo_core::paths::DB_FILE).is_file() {
        return Err(format!(
            "拒绝清理 {}：里面有一份稿库（{}），却不像验收沙箱。\
             验收模式绝不碰真稿库——请换一个空目录。",
            real.display(),
            yanmo_core::paths::DB_FILE
        ));
    }
    Ok(())
}

/// 这个目录算不算"验收自己的沙箱"（纯函数，便于单测）。
///
/// 两种算：① 带我们写的记号文件；② **就是那个默认沙箱路径**（`<临时目录>/yanmo-acceptance`）。
///
/// 为什么单列第 ② 条：0.50.0 及更早建的默认沙箱里没有记号文件，而"有稿库又无记号"那条规则
/// 会把老用户的验收工具直接卡死——升级一次版本不该让人连自检都跑不了。默认路径是**我们定义的**
/// 临时落点，按定义就是我们的（真稿库长在 `%TEMP%\yanmo-acceptance` 的概率可以忽略）。
///
/// 这条是**冒烟测试当场抓出来的**：新包跑默认路径返回了拒绝（退出码 4）。
fn is_our_scratch(real: &Path, temp: &Path, marked: bool) -> bool {
    marked || real == temp.join("yanmo-acceptance")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::acceptance::{run_bench, Plan};

    /// 一个"像真稿库"的目录：里面有库文件，还有一个作者自己放的备份包。
    /// 用 `TempDir` 自动清理——测试里不去碰 `remove_dir_all`（那正是本文件要盯住的动作）。
    fn library_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(yanmo_core::paths::DB_FILE), b"REAL LIBRARY - must survive")
            .unwrap();
        std::fs::write(dir.path().join("我的备份包.zip"), b"author's own backup").unwrap();
        dir
    }

    /// **这条就是那个不可恢复删库漏洞的守卫**：一个带稿库、却没有沙箱记号的目录，
    /// 哪怕它落在系统临时目录里，也一律拒绝——而且拒绝之后库必须原封不动。
    #[test]
    fn refuses_a_directory_that_holds_a_library() {
        let dir = library_dir();
        let refused = wipe_guard(dir.path()).unwrap_err();
        assert!(refused.contains("稿库"), "拒绝理由要让人看懂是稿库：{refused}");
        assert_eq!(
            std::fs::read(dir.path().join(yanmo_core::paths::DB_FILE)).unwrap(),
            b"REAL LIBRARY - must survive",
            "拒绝之后库文件必须一个字节都没动"
        );
    }

    /// 不在系统临时目录下、又没有记号：同样拒绝（`--dir` 指向别处时的那一路）。
    #[test]
    fn refuses_an_unmarked_directory_outside_temp() {
        let app_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let refused = wipe_guard(&app_dir).unwrap_err();
        assert!(
            refused.contains("临时目录"),
            "拒绝理由要说清「既不在临时目录、也没有记号」：{refused}"
        );
    }

    /// 第一次跑：目录还不存在 → 放行（没有东西可删，下面会建）。
    #[test]
    fn allows_a_directory_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("还没有这个目录");
        assert!(wipe_guard(&missing).is_ok(), "不存在的目录没有东西可删，该放行");
    }

    /// 自己造的沙箱（有记号）：里面有上一次验收留下的库也该能清掉。
    #[test]
    fn allows_a_marked_sandbox_even_with_a_library_inside() {
        let dir = library_dir();
        std::fs::write(dir.path().join(SCRATCH_MARKER), b"yanmo acceptance scratch\n").unwrap();
        assert!(wipe_guard(dir.path()).is_ok(), "上一次验收自己造的库，这次该能清");
    }

    /// 端到端：整轮跑在"看起来像真稿库"的目录上时，必须**拒绝执行**且什么都没动。
    #[test]
    fn a_refused_run_does_not_touch_the_directory() {
        let dir = library_dir();
        let plan = Plan { dir: dir.path().to_path_buf(), ..Plan::default() };

        let report = run_bench(&plan);

        assert!(report.refused.is_some(), "指向真稿库时必须拒绝执行");
        assert_eq!(
            std::fs::read(dir.path().join(yanmo_core::paths::DB_FILE)).unwrap(),
            b"REAL LIBRARY - must survive"
        );
        assert_eq!(
            std::fs::read(dir.path().join("我的备份包.zip")).unwrap(),
            b"author's own backup",
            "作者放在数据目录里的备份包也不许动"
        );
        assert!(
            report.steps.iter().any(|step| step.name == "造数据" && !step.note.is_empty()),
            "拒绝原因要写进报告"
        );
    }

    /// **默认沙箱路径按定义就是我们的**：0.50.0 建的它没有记号文件，升级之后也不该被卡死。
    /// 这条是冒烟测试当场抓出来的——新包跑默认路径返回了拒绝（退出码 4），
    /// 因为那个目录里还躺着上一次验收留下的库。
    #[test]
    fn the_default_scratch_path_is_ours_even_without_a_marker() {
        let temp = PathBuf::from("tmp-root");
        assert!(
            is_our_scratch(&temp.join("yanmo-acceptance"), &temp, false),
            "默认沙箱路径：没有记号也算我们的"
        );
        assert!(is_our_scratch(&temp.join("别处"), &temp, true), "有记号就算我们的");
        assert!(
            !is_our_scratch(&temp.join("我的稿子"), &temp, false),
            "临时目录里别的目录不算我们的（那可能真是稿库落在这儿了）"
        );
        assert!(
            !is_our_scratch(&temp.join("yanmo-acceptance-bak"), &temp, false),
            "名字像但不是那个路径，不算"
        );
    }
}
