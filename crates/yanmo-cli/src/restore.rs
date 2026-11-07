use serde_json::{json, Value};

use std::path::Path;

use yanmo_core::store::{preview_source, swap_in};

use crate::args::Args;
use crate::CliError;

/// 命令行救援：**把一份备份换回这个数据目录**。
///
/// 铁律（与界面里的恢复同一条）：默认只打印计划，`--yes` 才动手；动手前先留底
/// （`.db` 连同 `-wal` / `-shm` 一起搬进 `旧库留底/`），换完当场体检，失败回滚。
///
/// 这条命令**不打开活库**：库坏到打不开时，`Store::open` 根本起不来，而这正是它该干活的时刻。
pub(crate) fn run(args: &Args) -> Result<Value, CliError> {
    let from = Path::new(args.required("from")?);
    let data_dir = args.data.as_path();

    let preview = preview_source(from, data_dir).map_err(|error| {
        if error.code() == "backup.restore_source_invalid" {
            CliError::Refused { code: "backup.restore_source_invalid", detail: error.to_string() }
        } else {
            CliError::Core(error)
        }
    })?;
    if !preview.can_restore {
        return Err(CliError::Refused {
            code: "backup.restore_source_invalid",
            detail: preview
                .verify
                .problems
                .iter()
                .map(|problem| problem.render_log())
                .collect::<Vec<_>>()
                .join("；"),
        });
    }

    // 研墨还开着这个数据目录就绝不换库：两个写者抢同一个库是拿稿子冒险。
    if app_is_running(data_dir) {
        return Err(CliError::Refused {
            code: "backup.live_in_use",
            detail: "先把研墨关掉，再跑这条命令".to_string(),
        });
    }

    let plan = json!({
        "source": preview.source,
        "kind": preview.kind,
        "data_dir": data_dir.display().to_string(),
        "keep_dir": preview.keep_dir,
        "verify": preview.verify,
        "source_stats": {
            "created_at": preview.created_at,
            "engine_version": preview.engine_version,
            "data_format_version": preview.data_format_version,
            "works": preview.works,
            "chapters": preview.chapters,
            "words": preview.words,
            "bytes": preview.bytes,
        },
    });

    if !args.flag("yes") {
        return Ok(json!({
            "ok": true,
            "command": "restore",
            "dry_run": true,
            "plan": plan,
        }));
    }

    let stamp = format!("{}Z", yanmo_core::time::local_stamp(yanmo_core::time::now_millis(), 0));
    let outcome = swap_in(from, data_dir, &stamp)?;
    Ok(json!({
        "ok": true,
        "command": "restore",
        "dry_run": false,
        "outcome": outcome,
    }))
}

/// 这个数据目录是不是正被研墨占着（单实例锁还在）。
///
/// 锁是命名内核对象，名字由数据目录算出来（与壳同一份算法，见
/// `yanmo_core::paths::instance_lock_name`）；这里只问"这个名字在不在"。
#[cfg(windows)]
fn app_is_running(data_dir: &Path) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name = yanmo_core::paths::instance_lock_name(data_dir);
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, wide.as_ptr());
        if handle == std::ptr::null_mut() {
            // 建不出来（权限之类）：宁可让作者先关掉研墨，也不冒这个险
            return true;
        }
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        CloseHandle(handle);
        exists
    }
}

#[cfg(not(windows))]
fn app_is_running(_data_dir: &Path) -> bool {
    false
}
