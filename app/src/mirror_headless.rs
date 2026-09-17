//! 镜像的**无窗口驱动**：不开 GUI 把对账跑几轮，把结论写成 JSON。
//!
//! 它为什么单独一份：`mirror.rs` 管节拍与生命周期、`mirror_sync` 管"一轮对账怎么走"、
//! `mirror_fs` 管真碰磁盘——这一份管的既不是节拍也不是次序，而是**从外面驱动的形状**
//! （启动参数进来、自己开库、跑完给退出码与一份文件）。变化理由不同，所以是两件事。
//!
//! # 它跑的是不是"真那一步"
//!
//! 是。它调的是 [`crate::mirror_sync::sweep_data`]——**界面里那个后台线程调的同一个函数**：
//! 同一份计划、同一份账、同一条"外面的改动一个字不碰"。这里只补三件界面替它做的事：
//! 自己开库、按次序跑几轮、把结论落成文件（桌面程序在控制台里拿不到可用的标准输出，
//! 所以结论一律落文件）。
//!
//! # 幂等要有据可查
//!
//! 报告里的 `rounds` 是每一轮"有没有真往盘上写"：一轮**该写**、紧接着一轮**什么都不该动**。
//! 默认跑 2 轮就是这个意思——"第二次不生事"是这份报告里唯一能证明幂等的地方。
//!
//! # 退出码
//!
//! `0` 账对上了（没有对不上的书、没有错误、会话关干净了）；`1` 有书没对上或出过错；
//! `2` 连库都打不开（**没跑起来**，与"跑完发现不对"分开说）。

use std::path::Path;

use crate::mirror;
use crate::mirror_sync;
use crate::storage::AppData;

/// 跑 `rounds` 轮对账，结论写成 JSON；返回退出码。
///
/// `out` 不给就写在数据目录旁边（`yanmo-mirror.json`）——与库体检那条路同一个习惯。
pub fn run(data_dir: &Path, rounds: usize, out: Option<&Path>) -> i32 {
    let data = match AppData::open_for_acceptance(data_dir) {
        Ok(data) => data,
        Err(error) => {
            // i18n-allow-next-line: 命令行的机器可读输出（给脚本看），不是界面文案
            eprintln!("镜像对账没法开库：{error}");
            return 2;
        }
    };
    // 无工作线程的 Inner：这条路自己按次序跑，跑完立刻要结论（起线程反而多一层竞态）
    let inner = mirror::Inner::detached();
    let mut rounds_done: Vec<bool> = Vec::new();
    for _ in 0..rounds.max(1) {
        // full + idle：每一轮都**逐份核过磁盘**，不走"账上没变就跳过"那条快路
        rounds_done.push(mirror_sync::sweep_data(&data, &inner, true, true));
    }
    let status = inner.status();
    // **收尾必须把会话关干净**：开库就会登记一次会话（"上次退得干不干净"靠它），
    // 这一趟只对账、没碰正文，所以走"只标干净、不写快照"那条（与界面里"仍然退出"同一条）。
    // 不做这一步的话，作者下次打开会看到一句"上次不是正常退出"的**假警报**。
    let session_closed = data.with_store(|store| store.abandon_session()).is_ok();
    let worked = rounds_done.iter().filter(|did| **did).count();
    let ok = status.failed_works == 0 && status.last_error.is_empty() && session_closed;
    let json = format!(
        "{{\"ok\":{ok},\"rounds\":[{}],\"rounds_worked\":{worked},\"enabled\":{},\"files\":{},\
\"conflicts\":{},\"untracked\":{},\"failed_works\":{},\"session_closed\":{session_closed},\
\"last_sync_at\":{},\"last_error\":{}}}",
        rounds_done.iter().map(|did| did.to_string()).collect::<Vec<_>>().join(","),
        status.enabled,
        status.files,
        status.conflicts,
        status.untracked,
        status.failed_works,
        status.last_sync_at,
        crate::acceptance::json_string(&status.last_error),
    );
    let target = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| data_dir.join("yanmo-mirror.json"));
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&target, format!("{json}\n")) {
        // i18n-allow-next-line: 命令行的机器可读输出（给脚本看），不是界面文案
        eprintln!("镜像对账的结果没写成：{error}");
        return 2;
    }
    // 顺带写一份 stdout（从管道/文件重定向读的时候能看到；控制台里看不到是正常的）
    println!("{json}");
    if ok {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    //! 无窗口那条路的验收：**"外面驱动的就是 GUI 跑的那一套"要有据可查**。
    //!
    //! 两件事各自钉一条：① 一次对账把整本书写上盘，第二轮不再动它（幂等）；
    //! ② 从外面改过的那一份**一个字都不碰**、只如实报成待定夺——这条正是镜像对作者的承诺。

    use super::*;
    use std::path::PathBuf;

    use yanmo_core::model::{NodeKind, WorkKind};
    use yanmo_core::store::Store;

    const SENTENCE: &str = "雨下了整夜，屋檐上的水声一直没停。";

    /// 造一份带一本书两章的数据目录。
    fn seed(dir: &Path) {
        let mut store = Store::open(&dir.join(yanmo_core::paths::DB_FILE)).expect("开库");
        let work = store.create_work(WorkKind::Novel, "镜像用书").expect("建书");
        let volume = store
            .list_nodes(work.id)
            .expect("拉树")
            .first()
            .expect("新作品该有默认卷")
            .id;
        for index in 1..=2 {
            let chapter = store
                .create_node(work.id, Some(volume), NodeKind::Chapter, &format!("第 {index} 章"))
                .expect("建章");
            store.write_body(chapter, SENTENCE).expect("写正文");
        }
    }

    fn markdown_files(root: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "md") {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn writes_the_whole_book_then_settles() {
        let temp = tempfile::tempdir().expect("临时目录");
        seed(temp.path());
        assert_eq!(run(temp.path(), 2, None), 0, "对账该成功");

        let report = std::fs::read_to_string(temp.path().join("yanmo-mirror.json")).expect("报告");
        assert!(report.contains("\"ok\":true"), "{report}");
        assert!(report.contains("\"files\":2"), "账上该照看两章：{report}");
        assert!(
            report.contains("\"rounds\":[true,false]"),
            "第一轮写、第二轮一个字都不该再动（幂等）：{report}"
        );
        let files = markdown_files(&temp.path().join(mirror::MIRROR_DIR));
        assert_eq!(files.len(), 2, "磁盘上该有两份 .md：{files:?}");
    }

    #[test]
    fn leaves_the_session_clean_so_the_next_launch_is_not_a_false_alarm() {
        // 开库会登记一次会话；这一趟只对账、没碰正文，收尾必须关干净——
        // 否则作者下次打开会看到"上次不是正常退出"的假警报。
        // （这是按天连跑对账时真抓到的一条：开库即登记、退出没关。）
        let temp = tempfile::tempdir().expect("临时目录");
        seed(temp.path());
        assert_eq!(run(temp.path(), 2, None), 0);
        let report = std::fs::read_to_string(temp.path().join("yanmo-mirror.json")).expect("报告");
        assert!(report.contains("\"session_closed\":true"), "{report}");
        let store = Store::open(&temp.path().join(yanmo_core::paths::DB_FILE)).expect("开库");
        assert!(
            !store.peek_session().expect("看会话").unclean,
            "收尾之后会话必须是干净的"
        );
    }

    #[test]
    fn an_outside_edit_is_reported_and_never_overwritten() {
        let temp = tempfile::tempdir().expect("临时目录");
        seed(temp.path());
        assert_eq!(run(temp.path(), 2, None), 0);

        // 从外面改一份（记事本那条路）
        let files = markdown_files(&temp.path().join(mirror::MIRROR_DIR));
        assert_eq!(files.len(), 2);
        let mut text = std::fs::read_to_string(&files[0]).expect("读");
        text.push_str("\n外面加的一句话。\n");
        std::fs::write(&files[0], &text).expect("写");

        // 有冲突也要退 0：冲突是"请作者定夺"，不是对账失败
        assert_eq!(run(temp.path(), 2, None), 0);
        let report = std::fs::read_to_string(temp.path().join("yanmo-mirror.json")).expect("报告");
        assert!(report.contains("\"conflicts\":1"), "该如实报一份待定夺：{report}");
        assert!(
            std::fs::read_to_string(&files[0]).expect("读").contains("外面加的一句话"),
            "外面改过的那一份一个字都不许碰"
        );
    }
}
