//! 启动失败**说人话**：界面还没起来的时候，唯一还能跟作者说话的只有系统对话框。
//!
//! # 为什么中文写在这儿，而不是进前端字典
//!
//! 这一刻前端还不存在（窗口都没有），`frontend/src/locales/` 够不着。
//! 与 `single.rs` 那条「已经开着了」的弹窗同一条路数——所以整份文件豁免文案守卫。
//! i18n-allow-file: 启动期系统对话框（前端与字典都还没起来），见上面这段理由
//!
//! # 两条规矩
//!
//! 1. **任何一档都不许静默**：静默退出等于「双击没反应」——作者既不知道出了什么事，
//!    也不知道稿子还在不在，这是最难自查的一种坏法（实测过：库被占、只读、坏掉、
//!    以及记录指向的文件夹里没有库，这四种以前全是「双击没反应」）。
//! 2. **每一句都要配一条下一步**：只说「失败了」等于没说。作者要的答案只有两个——
//!    「我的稿子还在不在」「我现在该干什么」。
//!
//! 覆盖范围：`AppData::open` 这一路上会**挡住启动**的失败（见 `main.rs` 的 setup）。
//! 最后一行的错误码是**故意留着**的：这是作者唯一能转述给别人的原话。

use crate::error::ApiError;

/// 一条启动失败该怎么跟作者说。
struct Advice {
    /// 出了什么事（一句大白话）
    lead: &'static str,
    /// 现在该干什么（必须具体）
    next: &'static str,
}

/// 按错误码取说法。**认不出的码也要有话说**（退到通用那一条，并把原话带上）。
fn advice(code: &str) -> Advice {
    match code {
        "shell.already_running" => Advice {
            lead: "研墨已经在这个稿子目录里开着了。",
            next: "请用已经打开的那个窗口。如果确实要在这里再开一个，先把那一个关掉。",
        },
        "shell.data_dir_unavailable" => Advice {
            lead: "找不到该把稿子放在哪里——系统没有给出可用的应用数据目录。",
            next: "先确认系统盘的 AppData 目录还在；或者改用绿色版：绿色版的数据跟着程序走，\
                   不依赖系统目录。",
        },
        "shell.recorded_dir_missing_db" => Advice {
            lead: "记录里写的那个文件夹在，但里面没有稿库。",
            next: "那个文件夹多半被改名、挪走或清理工具删掉了。把它找回来（改回原来的名字与位置）\
                   再打开研墨；实在找不回来就用备份恢复。研墨不会在这里新建一个空库——\
                   那会让你以为稿子没了。",
        },
        "shell.data_dir_create_failed" => Advice {
            lead: "建不了稿子目录。",
            next: "看一眼那个位置：盘插着吗、这个文件夹让不让你写。稿子没有被改动过。",
        },
        "shell.data_dir_readonly" => Advice {
            lead: "稿子所在的文件夹写不进去。",
            next: "常见原因：盘写保护、权限不够、或者那个位置是只读的。稿子还在原处、一个字都没动\
                   ——把权限修好，或者换一个能写的位置再打开。",
        },
        "shell.db_open_failed" => Advice {
            lead: "打不开稿库。",
            next: "两种常见情形：① 库正被别的程序占着——先关掉它；② 库文件坏了——先别删，\
                   用备份恢复；没有备份的话，还可以试试把坏库挪开、开一个空库，再从备份包导入成稿。",
        },
        "shell.session_begin_failed" => Advice {
            lead: "稿库写不进去（登记本次会话失败）。",
            next: "多半是权限或磁盘空间的问题。稿子还在原处——先别继续写，处理好了再打开。",
        },
        _ => Advice {
            lead: "研墨没能启动。",
            next: "把下面这行原话记下来（含错误码）；如果还能打开别的库，先用它把稿子导出一份留底。",
        },
    }
}

/// 拼出对话框里的那段话：主句 → 位置 → 下一步 → 错误码与底层原话。
///
/// 纯函数（不起窗口），所以能单测——**这段话是启动失败时作者唯一的线索**，
/// 它错了等于我们没有报错。
pub fn explain(error: &ApiError) -> String {
    let advice = advice(&error.code);
    let mut lines = vec![advice.lead.to_string()];

    if let Some(path) = error.params.get("path") {
        lines.push(String::new());
        lines.push(format!("位置：{path}"));
    }
    if let Some(pointer) = error.params.get("pointer") {
        lines.push(format!("记录文件：{pointer}"));
    }

    lines.push(String::new());
    lines.push(advice.next.to_string());
    lines.push(String::new());
    lines.push(format!("（错误码：{}）", error.code));

    // 底层原话（多半是英文技术消息）：**只作最后一行**，供转述与查日志用。
    // `detail` 的形状是「码(参数) | 原因」，这里只取「原因」那半。
    if let Some((_, cause)) = error.detail.split_once(" | ") {
        if !cause.trim().is_empty() {
            lines.push(format!("（原话：{}）", cause.trim()));
        }
    }
    lines.join("\n")
}

/// 把这一档失败写到 stderr（给**脚本跑的路径**用：验收 / 演练要的是退出码，不是人去点确定）。
pub fn print_failure(error: &ApiError) {
    // 命令行/开发档跑的时候也该看得见：日志里留一份
    eprintln!("研墨启动失败：{}", error.code);
    eprintln!("{}", explain(error));
}

/// 把这一档失败告诉作者：stderr 留一份 + 弹一个原生对话框。**说完就干净退出**
///（调用方负责退出，别掉进 panic）。
///
/// ⚠️ 自动化路径（验收 / 演练）**别走这一条**——一个要人去点的框会把脚本挂在半路，
/// 那种"卡住"比失败还难查；那边用 [`print_failure`]。
pub fn announce_failure(error: &ApiError) {
    print_failure(error);
    let body = explain(error);
    // 「已经开着了」不是故障，用信息图标；其余都是真失败
    let kind = if error.code == "shell.already_running" { Kind::Info } else { Kind::Error };
    platform::message_box("研墨", &body, kind);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Info,
    Error,
}

#[cfg(windows)]
mod platform {
    use super::Kind;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
    };

    pub(super) fn message_box(title: &str, body: &str, kind: Kind) {
        let wide =
            |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
        let icon = if kind == Kind::Info { MB_ICONINFORMATION } else { MB_ICONERROR };
        let caption = wide(title);
        let message = wide(body);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                caption.as_ptr(),
                MB_OK | icon | MB_SETFOREGROUND,
            );
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Kind;

    /// 其余平台还没有发布形态：这里不弹窗（stderr 那一份照旧）。
    pub(super) fn message_box(_title: &str, _body: &str, _kind: Kind) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一条失败。参数用**临时目录拼出来的路径**——单测里不写任何真实盘符路径
    ///（公开内容里不该出现别人的机器长什么样）。
    fn failure(code: &'static str, params: &[(&'static str, &str)]) -> ApiError {
        ApiError::with(code, params.iter().map(|(key, value)| (*key, value.to_string())))
    }

    fn place(name: &str) -> String {
        std::env::temp_dir().join(name).display().to_string()
    }

    /// 挡住启动的每一档都要有**自己的**主句（不能全落到通用那句上）。
    #[test]
    fn every_startup_failure_has_its_own_words() {
        let codes = [
            "shell.already_running",
            "shell.data_dir_unavailable",
            "shell.recorded_dir_missing_db",
            "shell.data_dir_create_failed",
            "shell.data_dir_readonly",
            "shell.db_open_failed",
            "shell.session_begin_failed",
        ];
        let unknown = advice("shell.not_a_real_code");
        let mut seen: Vec<&str> = Vec::new();
        for code in codes {
            let advice = advice(code);
            assert!(!advice.lead.is_empty() && !advice.next.is_empty(), "{code} 缺主句或下一步");
            assert_ne!(advice.lead, unknown.lead, "{code} 落到了通用那句");
            seen.push(advice.lead);
        }
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before, "有两档的说法一模一样：{seen:?}");
    }

    /// 位置要出现在话里——"在哪出的问题"是作者第一个要问的。
    #[test]
    fn the_place_is_in_the_message() {
        let dir = place("yanmo-startup-library");
        let pointer = place("yanmo-location.txt");
        let missing = failure(
            "shell.recorded_dir_missing_db",
            &[("path", &dir), ("pointer", &pointer)],
        );
        let text = explain(&missing);
        assert!(text.contains(&dir), "缺路径：{text}");
        assert!(text.contains("yanmo-location.txt"), "缺位置记录：{text}");
    }

    /// 底层原话与错误码都要留下：那是作者唯一能转述的东西。
    #[test]
    fn the_code_and_the_original_wording_are_both_kept() {
        let db = place("yanmo.db");
        let locked = failure("shell.db_open_failed", &[("path", &db)])
            .caused_by("数据库错误：database is locked");
        let text = explain(&locked);
        assert!(text.contains("shell.db_open_failed"), "缺错误码：{text}");
        assert!(text.contains("database is locked"), "缺底层原话：{text}");
        assert!(!text.contains("shell.db_open_failed("), "别把「码(参数)」那半截也带上：{text}");
    }

    /// 认不出的码也要说人话（通用那句 + 码 + 原话），**不许空着**。
    ///
    /// 直接拼一个 `ApiError`（绕过 `with` 的"码必须已登记"断言）：这一档是**防御性**的——
    /// 万一将来冒出没登记的码，启动失败也不该只剩一片空白。
    #[test]
    fn an_unknown_code_still_says_something_useful() {
        let weird = ApiError {
            code: "shell.something_new".to_string(),
            params: Default::default(),
            detail: "shell.something_new | 底层原话".to_string(),
        };
        let text = explain(&weird);
        assert!(text.contains("shell.something_new"), "认不出的码要原样带出来：{text}");
        assert!(text.contains("没能启动"), "至少要说清是启动失败：{text}");
        assert!(text.lines().count() > 2, "别只剩一行：{text}");
        assert!(text.contains("底层原话"), "原话那一行也要在：{text}");
    }
}
