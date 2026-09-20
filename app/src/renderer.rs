//! 精准挑出本程序 WebView2 的**渲染进程**：纯逻辑那一半（不碰任何 Windows API）。
//!
//! # 口径从哪来（真机两轮演练实测，不是想当然）
//!
//! 2026-09-17 往真页面注入一个 `while(true){}`，把六种"复活"手段逐个试完，只有一根管用。
//! 这里照抄的就是那一根的口径：
//!
//! 1. **先找浏览器进程**：`msedgewebview2.exe` 里**父进程是我们自己**的那一个
//!    （界面的宿主；它才是"我们这扇窗"的那棵树）；
//! 2. **再找它下面的 renderer**：同样是 `msedgewebview2.exe`、父进程是那个浏览器进程，
//!    且命令行里带着 `--type=renderer`；
//! 3. **只杀 renderer，浏览器进程留着**——WebView2 自己会拉起新的渲染进程并把页面重新挂上
//!    （实测 10 秒内回来）。杀浏览器进程、杀它下面**所有**子进程、整扇窗口重建都试过，
//!    **全都更糟**（GPU / 网络被捎带杀掉就再也回不来）。
//!
//! # 为什么这一半单独成件、且一行 Windows API 都不碰
//!
//! 要测死的是**挑的规则**，不是 syscall：别的程序的 WebView2 一个都不许碰、GPU / 网络进程
//! 不许碰、命令行读不出来时**宁可不杀**、没有 renderer 时如实说没有。这些用一棵假进程树
//! 就能全测到，不必起真的 WebView2（那一档是演练的事）。
//! FFI 那一半在 [`crate::renderer_win`]：它只负责"把进程树捞回来、把命令行读出来、把 pid 杀掉"。

/// 进程树里的一行（`CreateToolhelp32Snapshot` 能给的就这些）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: u32,
    /// 可执行文件名（`msedgewebview2.exe` 这种；不含路径）
    pub exe: String,
}

/// WebView2 的宿主进程名（**大小写不敏感**地比：Windows 上文件名本来就不分大小写）。
const WEBVIEW_EXE: &str = "msedgewebview2.exe";

/// Chromium 用 `--type=` 把子进程分家：`renderer` / `gpu-process` / `utility` / `crashpad-handler`…
const RENDERER_TYPE: &str = "--type=renderer";

/// 我们自己下面那个 WebView2 **浏览器进程**。
///
/// 没有就 `None`：界面还没挂起来，或者它已经没了（那一档我们救不了，也不硬试——
/// 见 [`crate::guardian`] 里那张实测表）。
/// 真出现多个（罕见）取 **pid 最小的那个**：确定性，而且一次只动一棵树。
pub fn webview_browser(procs: &[ProcInfo], me: u32) -> Option<u32> {
    procs
        .iter()
        .filter(|proc| proc.parent == me && same_exe(&proc.exe, WEBVIEW_EXE))
        .map(|proc| proc.pid)
        .min()
}

/// 那个浏览器进程下面**名字与父进程都对得上**的候选（命令行还没读）。
pub fn browser_children(procs: &[ProcInfo], browser: u32) -> Vec<u32> {
    let mut out: Vec<u32> = procs
        .iter()
        .filter(|proc| proc.parent == browser && same_exe(&proc.exe, WEBVIEW_EXE))
        .map(|proc| proc.pid)
        .collect();
    out.sort_unstable();
    out
}

/// 命令行像不像一个**渲染进程**。
///
/// 按**整词**比，不是子串：`--type=renderer` 与 `--type=renderer-xxx` 是两回事，
/// 将来 Chromium 多出一个近亲也不会被误当成渲染进程。认不出来就不杀（退回软重载）。
pub fn is_renderer(command_line: &str) -> bool {
    command_line
        .split_whitespace()
        .any(|token| token.trim_matches('"') == RENDERER_TYPE)
}

/// **整条规则**：从一棵进程树里挑出该杀的那几个渲染进程（按 pid 升序，可复核）。
///
/// `command_line` 由调用方给（真机上它读的是 `NtQueryInformationProcess`；测试里给的是一张表）。
/// 读不出来的那一个**跳过**——宁可不杀，也不误杀一个来路不明的进程。
pub fn renderers_to_kill(
    procs: &[ProcInfo],
    me: u32,
    command_line: &dyn Fn(u32) -> Option<String>,
) -> Vec<u32> {
    let Some(browser) = webview_browser(procs, me) else {
        return Vec::new();
    };
    let mut out: Vec<u32> = browser_children(procs, browser)
        .into_iter()
        .filter(|pid| command_line(*pid).is_some_and(|line| is_renderer(&line)))
        .collect();
    out.sort_unstable();
    out
}

/// 文件名比对（大小写不敏感；库里给的是文件名不是路径，不猜路径）。
fn same_exe(name: &str, wanted: &str) -> bool {
    name.eq_ignore_ascii_case(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, exe: &str) -> ProcInfo {
        ProcInfo { pid, parent, exe: exe.to_string() }
    }

    /// 一棵正常的树：我们（100）→ WebView2 浏览器（200）→ renderer（201/202）、
    /// GPU（203）、网络（204）。
    fn normal_tree() -> Vec<ProcInfo> {
        vec![
            proc(100, 4, "yanmo.exe"),
            proc(200, 100, "msedgewebview2.exe"),
            proc(201, 200, "msedgewebview2.exe"),
            proc(202, 200, "msedgewebview2.exe"),
            proc(203, 200, "msedgewebview2.exe"),
            proc(204, 200, "msedgewebview2.exe"),
        ]
    }

    fn lines(pairs: &[(u32, &str)]) -> impl Fn(u32) -> Option<String> + use<> {
        let table: Vec<(u32, String)> =
            pairs.iter().map(|(pid, line)| (*pid, line.to_string())).collect();
        move |pid: u32| {
            table.iter().find(|(id, _)| *id == pid).map(|(_, line)| line.clone())
        }
    }

    #[test]
    fn only_the_renderers_of_our_own_browser_process_are_picked() {
        let table = lines(&[
            (201, r#""msedgewebview2.exe" --type=renderer --foo"#),
            (202, r#""msedgewebview2.exe" --type=renderer --bar"#),
            (203, r#""msedgewebview2.exe" --type=gpu-process"#),
            (204, r#""msedgewebview2.exe" --type=utility --utility-sub-type=network"#),
        ]);
        assert_eq!(renderers_to_kill(&normal_tree(), 100, &table), vec![201, 202]);
    }

    /// 别的程序的 WebView2 **一个都不许碰**（它不挂在我们下面）。
    #[test]
    fn another_apps_webview_is_untouchable() {
        let procs = vec![
            proc(100, 4, "yanmo.exe"),
            proc(900, 500, "msedgewebview2.exe"), // 别人的浏览器进程
            proc(901, 900, "msedgewebview2.exe"), // 别人的 renderer
        ];
        let table = lines(&[(901, "msedgewebview2.exe --type=renderer")]);
        assert!(renderers_to_kill(&procs, 100, &table).is_empty());
    }

    /// 我们自己的浏览器进程没起来 / 已经没了 ⇒ 挑不出来（如实说"没有"，不硬杀）。
    #[test]
    fn without_our_browser_process_there_is_nothing_to_pick() {
        let procs = vec![proc(100, 4, "yanmo.exe"), proc(300, 4, "explorer.exe")];
        assert_eq!(webview_browser(&procs, 100), None);
        assert!(renderers_to_kill(&procs, 100, &lines(&[])).is_empty());
    }

    /// 浏览器进程在，但下面**一个 renderer 都没有**（只有 GPU 那些）⇒ 不杀。
    #[test]
    fn siblings_that_are_not_renderers_are_left_alone() {
        let procs = vec![
            proc(100, 4, "yanmo.exe"),
            proc(200, 100, "msedgewebview2.exe"),
            proc(203, 200, "msedgewebview2.exe"),
        ];
        let table = lines(&[(203, "msedgewebview2.exe --type=gpu-process")]);
        assert!(renderers_to_kill(&procs, 100, &table).is_empty(), "GPU 进程不许碰");
    }

    /// 命令行**读不出来**的那一个跳过（宁可不杀，也不误杀）。
    #[test]
    fn an_unreadable_command_line_is_skipped() {
        let table = lines(&[(202, "msedgewebview2.exe --type=renderer")]);
        assert_eq!(
            renderers_to_kill(&normal_tree(), 100, &table),
            vec![202],
            "能读的那个照杀，读不出的那个不猜"
        );
    }

    /// 真出现两个浏览器进程（罕见）时只动 pid 最小的那一棵（确定性 + 不扩大打击面）。
    #[test]
    fn two_browser_processes_pick_the_smallest_and_only_its_tree() {
        let procs = vec![
            proc(100, 4, "yanmo.exe"),
            proc(200, 100, "msedgewebview2.exe"),
            proc(201, 200, "msedgewebview2.exe"),
            proc(400, 100, "msedgewebview2.exe"),
            proc(401, 400, "msedgewebview2.exe"),
        ];
        let table = lines(&[
            (201, "msedgewebview2.exe --type=renderer"),
            (401, "msedgewebview2.exe --type=renderer"),
        ]);
        assert_eq!(webview_browser(&procs, 100), Some(200));
        assert_eq!(renderers_to_kill(&procs, 100, &table), vec![201]);
    }

    #[test]
    fn the_renderer_flag_is_matched_as_a_whole_word() {
        assert!(is_renderer("msedgewebview2.exe --type=renderer"));
        assert!(is_renderer(r#""msedgewebview2.exe" --type=renderer --foo=1"#));
        assert!(!is_renderer("msedgewebview2.exe --type=renderer-helper"), "近亲不算");
        assert!(!is_renderer("msedgewebview2.exe --type=gpu-process"));
        // 名字大小写不敏感（Windows 上文件名本来就不分大小写）
        assert!(same_exe("MSEDGEWEBVIEW2.EXE", WEBVIEW_EXE));
        let procs = vec![proc(100, 4, "yanmo.exe"), proc(200, 100, "MSEDGEWEBVIEW2.EXE")];
        assert_eq!(webview_browser(&procs, 100), Some(200));
    }
}
