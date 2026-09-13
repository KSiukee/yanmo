//! 单实例守卫：**同一个数据目录，同一时刻只允许一个研墨**。
//!
//! # 为什么按「数据目录」而不是按整机
//!
//! 真正会撞坏库的是「两个进程同时写同一个 `yanmo.db`」。数据目录不同（便携库、将来的
//! 多库、演练用的沙盒）就该各开各的，互不打扰。所以锁的名字**由数据目录算出来**，
//! 而且只放一个定长指纹进去——不把作者的目录名挂到系统对象名里给别的进程看。
//!
//! # 为什么用命名互斥体，不用锁文件
//!
//! **进程被杀、崩溃、断电时句柄由系统收走，锁自动消失。** 锁文件会在崩溃后留下一具尸体：
//! 下次启动要么误报「已经在运行」（软件再也打不开），要么得靠 PID 猜它死没死——两种都不要。
//! 判据只有一条：`CreateMutexW` 之后 `GetLastError() == ERROR_ALREADY_EXISTS`。
//!
//! # 为什么要「等一小会儿」
//!
//! 两条真实路径：① 刚关掉又马上打开；② **换库之后壳自己重启**——重启是先起新进程、
//! 旧进程随后退出，新进程可能比旧进程更早来要锁。等最多一秒就能安然度过；
//! 而真正的「第二个研墨」只是多等这一秒，然后看见一句人话提示。

use std::path::Path;
use std::time::Duration;

/// 重试间隔与次数（最多等 `WAIT_STEP × WAIT_TRIES` = 1 秒）。
const WAIT_STEP: Duration = Duration::from_millis(100);
const WAIT_TRIES: u32 = 10;

/// 一把占着的实例锁：**活着就占着**（drop 或进程结束才松开）。
pub struct InstanceLock {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// 安全：这把锁只持有一个内核句柄，句柄本身与线程无关（`CloseHandle` 可以从任何线程调），
// 我们也不跨线程共享它——只是允许"谁先收尾谁关"（换库重启那条路要把锁交给后台线程）。
unsafe impl Send for InstanceLock {}

impl InstanceLock {
    /// 试一次：这个数据目录已经有人占着就返回 `Err(())`。
    pub fn try_acquire(data_dir: &Path) -> Result<Self, ()> {
        imp::try_acquire(data_dir)
    }

    /// 等一小会儿再试（给「刚关掉又马上打开」和「换库重启」留出窗口）。
    pub fn acquire(data_dir: &Path) -> Result<Self, ()> {
        let mut last = Err(());
        for attempt in 0..WAIT_TRIES {
            last = Self::try_acquire(data_dir);
            if last.is_ok() {
                return last;
            }
            if attempt + 1 < WAIT_TRIES {
                std::thread::sleep(WAIT_STEP);
            }
        }
        last
    }
}

/// 告诉作者「这个数据目录已经开着一个研墨了」。
///
/// 应用起不来时**界面还不存在**，只能弹系统对话框——与 `main.rs` 里那句启动失败文本同理。
pub fn announce_already_running(data_dir: &str) {
    imp::announce(data_dir);
}

#[cfg(windows)]
impl Drop for InstanceLock {
    fn drop(&mut self) {
        imp::close(self);
    }
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
    };

    use super::InstanceLock;

    /// 锁的名字：固定前缀 + 数据目录指纹。
    ///
    /// 先取规范路径再算指纹：`文档\研墨` 与 `文档\..\文档\研墨` 必须算出同一个名字，
    /// 否则换个写法就能同时开两个（Windows 上大小写也会被 canonicalize 归一）。
    fn lock_name(data_dir: &Path) -> Vec<u16> {
        let canonical = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        let fingerprint = yanmo_core::text::content_hash(&canonical.to_string_lossy());
        format!("Local\\app.yanmo.desktop.data-{fingerprint}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn try_acquire(data_dir: &Path) -> Result<InstanceLock, ()> {
        let name = lock_name(data_dir);
        // `binitialowner = 0`：不申请持有——我们只用「这个名字在不在」当判据。
        // 不申请持有还带来一件事：最后一个句柄关闭时系统就销毁它（崩溃自动解锁靠这个）。
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            // 建不出来（权限之类）：宁可这次启动失败，也不装作拿到了锁
            return Err(());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Err(());
        }
        Ok(InstanceLock { handle })
    }

    pub(super) fn close(lock: &mut InstanceLock) {
        if !lock.handle.is_null() {
            unsafe { CloseHandle(lock.handle) };
            lock.handle = std::ptr::null_mut();
        }
    }

    pub(super) fn announce(data_dir: &str) {
        let wide =
            |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
        // i18n-allow-next-line: 启动期系统对话框——前端还没起来，没有字典可查（与 main.rs 的启动失败文本同理）
        let message = format!(
            "研墨已经在这个数据目录里开着了：\n{data_dir}\n\n请用已经打开的那个窗口。如果只想在这里再开一个，先把那一个关掉。"
        );
        // i18n-allow-next-line: 产品名（品牌），不翻译
        let title = wide("研墨");
        let body = wide(&message);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
            );
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    use super::InstanceLock;

    pub(super) fn try_acquire(_data_dir: &Path) -> Result<InstanceLock, ()> {
        // 其余平台还没有发布形态——先不装守卫（行为与从前一致）。等那些平台要发布时，
        // 在这里换成对应的文件锁/命名内核对象即可，接口这一层不用动。
        Ok(InstanceLock {})
    }

    pub(super) fn announce(_data_dir: &str) {}
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn one_lock_per_data_directory_and_it_comes_back_after_release() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();

        let first = InstanceLock::try_acquire(dir.path()).unwrap();
        assert!(
            InstanceLock::try_acquire(dir.path()).is_err(),
            "同一个数据目录不许开第二个研墨"
        );
        assert!(
            InstanceLock::try_acquire(other.path()).is_ok(),
            "别的数据目录互不打扰（便携库 / 多库 / 演练沙盒都要能同时开）"
        );

        drop(first);
        assert!(InstanceLock::try_acquire(dir.path()).is_ok(), "松开之后要能再拿到");
    }

    #[test]
    fn acquire_waits_for_a_lock_that_is_about_to_be_released() {
        let dir = tempfile::tempdir().unwrap();
        let held = InstanceLock::try_acquire(dir.path()).unwrap();
        let path = dir.path().to_path_buf();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
        });

        let started = std::time::Instant::now();
        let again = InstanceLock::acquire(&path).expect("等一会儿就该拿到（刚关掉又打开靠这条）");
        assert!(started.elapsed() >= Duration::from_millis(200), "确实等了，而不是立刻拿到");
        releaser.join().unwrap();
        drop(again);
    }

    #[test]
    fn a_relative_spelling_of_the_same_directory_is_the_same_lock() {
        let dir = tempfile::tempdir().unwrap();
        let held = InstanceLock::try_acquire(dir.path()).unwrap();
        // 同一个目录换一种写法（多一段 `..`）必须还是同一把锁
        let alias = dir.path().join("..").join(dir.path().file_name().unwrap());
        assert!(
            InstanceLock::try_acquire(&alias).is_err(),
            "换个写法就绕开锁，等于没锁：{}",
            alias.display()
        );
        drop(held);
    }
}
