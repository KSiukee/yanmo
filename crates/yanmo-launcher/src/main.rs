//! `yanmo-launch`：把「起核心 IPC 服务 + 拉宿主 + 等退出」做成一个**独立进程**。
//!
//! 壳只负责按约定路径拉起它；它自己生成一次性 token（stdin 交给宿主），
//! 不把 token 放进命令行。参数全部是路径/秒数，不经过界面。
//!
//! ```text
//! yanmo-launch --db <yanmo.db> --module <受管模块目录> --host <yanmo-host 可执行>
//!              [--timeout-secs 60]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::json;
use yanmo_launcher::{launch, LaunchError, LaunchRequest};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                json!({ "ok": false, "code": error.code, "detail": error.detail })
            );
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), LaunchError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = value_of(&args, "--db")
        .map(PathBuf::from)
        .ok_or_else(|| LaunchError::new(LaunchError::DB, "missing --db <file>"))?;
    let module = value_of(&args, "--module")
        .map(PathBuf::from)
        .ok_or_else(|| LaunchError::new(LaunchError::MANIFEST, "missing --module <dir>"))?;
    let host = value_of(&args, "--host")
        .map(PathBuf::from)
        .ok_or_else(|| LaunchError::new(LaunchError::SPAWN, "missing --host <file>"))?;
    let resident = args.iter().any(|arg| arg == "--resident");
    let timeout = value_of(&args, "--timeout-secs")
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.clamp(1, 3600)))
        .unwrap_or(Duration::from_secs(60));

    let report = launch(&LaunchRequest {
        db_path: db,
        module_dir: module,
        host_bin: host,
        timeout,
        resident,
    })?;
    if resident {
        // 宿主自己的报告直接走继承的 stdout；这里只把退出码变成进程退出码。
        if report.timed_out || report.exit_code.unwrap_or(1) != 0 {
            return Err(LaunchError::new(
                LaunchError::WAIT,
                format!("host exit {:?}", report.exit_code),
            ));
        }
        return Ok(());
    }
    let text = serde_json::to_string(&report)
        .map_err(|error| LaunchError::new(LaunchError::WAIT, error.to_string()))?;
    println!("{text}");
    Ok(())
}

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}
