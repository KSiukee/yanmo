//! 精准挑渲染进程的 **Windows 那一半**：捞进程树、读命令行、杀那几个 pid。
//!
//! 挑的规则（谁该杀、谁一个都不许碰）全在 [`crate::renderer`] 里——这一件只做三件事：
//! `CreateToolhelp32Snapshot` 捞树、`NtQueryInformationProcess` 读命令行、`TerminateProcess` 杀 pid。
//!
//! # 两个讲究
//!
//! 1. **不加依赖、也不手工绑 ntdll**：用的是现成的 `windows-sys`（本来就是本壳的依赖：
//!    卷标探测、内存总量、选文件夹对话框都在用），这里只多开了三个 feature——
//!    `Win32_System_Diagnostics_ToolHelp`（进程树）与 `Wdk_System_Threading`（读另一个进程的命令行）
//!    ——杀进程用的 `TerminateProcess` 本来就在已经开着的 `Win32_System_Threading` 里。
//! 2. **只杀命令行里认得出是 renderer 的那些**（见 [`crate::renderer::renderers_to_kill`]）：
//!    一个都认不出来就返回 `Err`，由调用方退回软重载——**绝不"按名字乱杀"**。
//!    真机演练里"杀浏览器进程"与"杀它下面所有子进程"都试过，**全都更糟**（GPU / 网络被捎带杀
//!    掉之后页面再也回不来），所以这里只动那一个 `--type=renderer`。
//!
//! 命令行那一处用的是内核接口（`NtQueryInformationProcess`）：它要先把缓冲区开小了问一次长度、
//! 再开够问第二次——第一次返回"长度不够"是**正常流程**，不是错误。

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    use windows_sys::Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessCommandLineInformation,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_TERMINATE,
    };

    use crate::renderer::{renderers_to_kill, ProcInfo};

    /// 一次"复活"要用的 pid 上限（只是防手改出来的天文数字，正常树个位数）。
    const MAX_TARGETS: usize = 8;
    /// 命令行缓冲区的上限（Chromium 的命令行可以很长，8K 在真机上足够；超了就放弃这一头）。
    const MAX_COMMAND_LINE_BYTES: u32 = 8 * 1024;

    /// 命令行从内核返回来的那一小段头（`UNICODE_STRING`）+ 紧跟其后的字符串本体。
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }

    /// **把渲染进程杀掉**（浏览器进程留着），返回一句给日志/体检看的话。
    ///
    /// 挑不出来就 `Err`（调用方退回软重载）；挑出来了但一个都没杀成，也算 `Err` 并带上原因。
    pub fn kill_renderers() -> Result<String, String> {
        let procs = snapshot();
        if procs.is_empty() {
            return Err("CreateToolhelp32Snapshot failed (no process tree)".to_string());
        }
        let me = unsafe { GetCurrentProcessId() };
        let mut targets = renderers_to_kill(&procs, me, &command_line_of);
        targets.truncate(MAX_TARGETS);
        if targets.is_empty() {
            // 英文：诊断串给开发者与体检看，不是界面文案（与 guardian 里那些 note 同一条）
            return Err("no WebView2 renderer process found for this app".to_string());
        }
        let mut killed: Vec<u32> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for pid in targets {
            match terminate(pid) {
                Ok(()) => killed.push(pid),
                Err(why) => failed.push(format!("{pid} ({why})")),
            }
        }
        if killed.is_empty() {
            return Err(format!("no renderer process could be killed: {}", failed.join(", ")));
        }
        let note = if failed.is_empty() {
            String::new()
        } else {
            format!("; {} could not be killed", failed.join(", "))
        };
        Ok(format!(
            "killed renderer process(es) {killed:?}{note} \
             (browser process left alone; WebView2 re-mounts the page itself)"
        ))
    }

    /// 捞一棵进程树（pid + 父 pid + 可执行文件名）。
    fn snapshot() -> Vec<ProcInfo> {
        let mut out = Vec::new();
        let snap: HANDLE = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = unsafe { Process32FirstW(snap, &mut entry) } != 0;
        while ok {
            out.push(ProcInfo {
                pid: entry.th32ProcessID,
                parent: entry.th32ParentProcessID,
                exe: exe_name(&entry),
            });
            ok = unsafe { Process32NextW(snap, &mut entry) } != 0;
        }
        unsafe { CloseHandle(snap) };
        out
    }

    /// `szExeFile` 是一个定长 UTF-16 数组，取到第一个 NUL 为止。
    fn exe_name(entry: &PROCESSENTRY32W) -> String {
        let end = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        String::from_utf16_lossy(&entry.szExeFile[..end])
    }

    /// 读另一个进程的命令行；读不出来给 `None`（**不猜**——调用方会跳过它）。
    fn command_line_of(pid: u32) -> Option<String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let text = unsafe { query_command_line(handle) };
        unsafe { CloseHandle(handle) };
        text
    }

    /// 两次调用：先问长度、再读内容（第一次返回"长度不够"是正常流程）。
    unsafe fn query_command_line(handle: HANDLE) -> Option<String> {
        let mut needed: u32 = 0;
        unsafe {
            NtQueryInformationProcess(
                handle,
                ProcessCommandLineInformation,
                std::ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        if needed == 0 || needed > MAX_COMMAND_LINE_BYTES {
            return None;
        }
        let mut buffer: Vec<u8> = vec![0u8; needed as usize + 2];
        let status = unsafe {
            NtQueryInformationProcess(
                handle,
                ProcessCommandLineInformation,
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer.len() as u32,
                &mut needed,
            )
        };
        if status != 0 {
            return None;
        }
        // 缓冲区开头是 UNICODE_STRING，它的 Buffer 指回这段缓冲区里的字符串本体
        let header = unsafe { &*(buffer.as_ptr().cast::<UnicodeString>()) };
        if header.buffer.is_null() || header.length == 0 {
            return None;
        }
        let units = unsafe { std::slice::from_raw_parts(header.buffer, (header.length / 2) as usize) };
        Some(String::from_utf16_lossy(units))
    }

    /// 杀一个 pid（尽力而为：失败把原因带回去，不影响别的目标）。
    fn terminate(pid: u32) -> Result<(), String> {
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err("OpenProcess failed".to_string());
        }
        let ok = unsafe { TerminateProcess(handle, 1) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return Err("TerminateProcess failed".to_string());
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 真机那一小块：**进程树捞得回来、命令行读得出来**（读我们自己的就够了——
        /// 挑的规则全在 `crate::renderer` 里用假树测死）。
        #[test]
        fn the_snapshot_and_the_command_line_reader_work_on_this_machine() {
            let me = unsafe { GetCurrentProcessId() };
            let procs = snapshot();
            assert!(procs.len() > 1, "这台机器上总不止一个进程：{}", procs.len());
            let mine = procs.iter().find(|proc| proc.pid == me).expect("该能捞到我们自己");
            assert!(mine.exe.to_ascii_lowercase().ends_with(".exe"), "exe 名：{}", mine.exe);

            let line = command_line_of(me).expect("读自己的命令行不该失败");
            assert!(
                line.contains(&mine.exe) || line.to_ascii_lowercase().contains(".exe"),
                "读回来的命令行看着不像我们自己的：{line}"
            );
        }

        /// 安全底线：测试进程下面没有 WebView2 ⇒ 一个都不挑（**绝不乱杀**）。
        #[test]
        fn nothing_is_picked_when_we_have_no_webview() {
            let me = unsafe { GetCurrentProcessId() };
            let procs = snapshot();
            let targets = renderers_to_kill(&procs, me, &command_line_of);
            assert!(targets.is_empty(), "测试进程哪儿来的渲染进程：{targets:?}");
        }
    }
}

#[cfg(windows)]
pub use imp::kill_renderers;

/// 别的平台没有 WebView2：这一根杠杆用不了（`guardian` 会因此退回软重载）。
#[cfg(not(windows))]
pub fn kill_renderers() -> Result<String, String> {
    Err("WebView2 only exists on Windows".to_string())
}
