//! 宿主的**作业对象**（Windows）：单进程 + 内存上限 + 句柄关闭即杀。
//!
//! 非 Windows 上是空实现：启动仍然可用，只是没有 OS 级配额（宿主自己的 fuel /
//! `StoreLimits` 仍然是硬的）。

#[cfg(windows)]
mod imp {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };

    /// 一个作业对象；`Drop` 时关句柄（`KILL_ON_JOB_CLOSE` 会把还活着的子进程收掉）。
    pub struct JobGuard {
        handle: HANDLE,
    }

    impl JobGuard {
        /// 建作业对象并设上限；失败返回 `None`（启动照常，只是没有配额）。
        pub fn create(memory_limit: usize) -> Option<Self> {
            Self::create_with_limit(memory_limit, 1)
        }

        /// 平台模式：允许多个模块子进程共享同一个作业。
        pub fn create_with_limit(memory_limit: usize, process_limit: usize) -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                    | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                    | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
                info.BasicLimitInformation.ActiveProcessLimit = process_limit.max(1) as u32;
                info.ProcessMemoryLimit = memory_limit;
                let ok = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(Self { handle })
            }
        }

        /// 把已经拉起的子进程挂进作业；失败（例如父进程本身在受限作业里）返回 false。
        pub fn assign(&self, child: &Child) -> bool {
            unsafe {
                AssignProcessToJobObject(
                    self.handle,
                    child.as_raw_handle() as HANDLE,
                ) != 0
            }
        }

        /// 立刻收掉作业里的所有进程。
        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.handle, 1);
            }
        }
    }

    impl Drop for JobGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Child;

    /// 非 Windows：没有作业对象；启动照常，配额只剩宿主自己的 fuel / 内存上限。
    pub struct JobGuard;

    impl JobGuard {
        pub fn create(_memory_limit: usize) -> Option<Self> {
            None
        }

        pub fn create_with_limit(_memory_limit: usize, _process_limit: usize) -> Option<Self> {
            None
        }

        pub fn assign(&self, _child: &Child) -> bool {
            false
        }

        pub fn terminate(&self) {}
    }
}

pub use imp::JobGuard;

#[cfg(all(test, windows))]
mod tests {
    use std::process::{Command, Stdio};

    use super::JobGuard;

    #[test]
    fn a_job_can_hold_a_child() {
        let mut child = Command::new("cmd")
            .args(["/c", "exit", "0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let Some(guard) = JobGuard::create(64 * 1024 * 1024) else {
            let _ = child.wait();
            return;
        };
        if !guard.assign(&child) {
            let _ = child.wait();
            return; // 父进程本身在受限作业里时挂不上；跳过，不当失败
        }
        assert!(child.wait().unwrap().success());
    }
}
