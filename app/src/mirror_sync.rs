//! 磁盘 `.md` 强镜像的**一轮对账**：取数 → 算计划 → 落盘 → 记账 → 收集"要作者定夺的事"。
//!
//! 单独一份的理由：线程与生命周期（[`crate::mirror`]）和"这一趟具体怎么走"是两件事——
//! 前者管节拍、停机、报告结构，后者管每一步的次序与出错时怎么办。混在一起，改任何一头
//! 都要把另一头重读一遍。
//!
//! # 锁的纪律
//!
//! **锁内取数与记账、锁外读写文件**：文件 I/O 不许占着数据锁，那是击键要走的路。
//!
//! # 出错怎么办
//!
//! 单本书出问题（树深过上限、库暂时关着）**不该让别的书跟着停摆**：记一笔、接着往下走，
//! 下一轮再来。落盘没成就**不记账**——账留在原样，下一轮从原样重来（写是幂等的，重来不亏）。

use std::collections::HashMap;

use tauri::{AppHandle, Manager};
use yanmo_core::store::MirrorEntry;

use crate::error::ApiError;
use crate::mirror::{Inner, ISSUE_LIST_MAX, MirrorIssue, MirrorStatus};
use crate::mirror_fs::{disk_looks_untouched, execute, probe};
use crate::storage::AppData;

pub(crate) fn sweep(app: &AppHandle, inner: &Inner, full: bool, idle: bool) -> bool {
    let Some(data) = app.try_state::<AppData>() else {
        return false;
    };
    let root = data.mirror_root();
    let mut report = MirrorStatus {
        root: root.display().to_string(),
        ..MirrorStatus::default()
    };
    // 上一轮"没账的文件"清单：只在空闲 / 全量那几轮才真去扫目录，其余几轮沿用
    // （扫目录是 FS 走一遍，不能每投一次信号就来一次）
    let carried_untracked = inner.previous_untracked();

    match data.with_store(|store| store.mirror_enabled()) {
        Ok(enabled) => report.enabled = enabled,
        Err(error) => {
            report.last_error = technical(&error);
            inner.publish(report);
            return false;
        }
    }
    if !report.enabled {
        report.last_sync_at = yanmo_core::time::now_millis();
        inner.publish(report);
        return false;
    }

    let works = match data.with_store(|store| store.mirror_works()) {
        Ok(works) => works,
        Err(error) => {
            report.last_error = technical(&error);
            inner.publish(report);
            return false;
        }
    };

    let mut did_work = false;
    for work in works {
        // ① 锁内：算出"该有什么"和"账上是什么"（不碰文件）
        let prepared = data.with_store(|store| {
            Ok((
                store.render_mirror(work)?,
                store.mirror_state(work)?,
                store.mirror_pending(work)?,
            ))
        });
        let (desired, state, pending) = match prepared {
            Ok(parts) => parts,
            Err(error) => {
                // 单本书出问题（比如树深过上限）不该让别的书跟着停摆
                report.failed_works += 1;
                report.last_error = technical(&error);
                continue;
            }
        };

        let mut verify = full;
        if !pending {
            // 库没变：日常投来的信号到这儿就结束；只有空闲巡检 / 全量核对才去看磁盘
            if !(idle || full) {
                report.files += state.len();
                continue;
            }
            if disk_looks_untouched(&root, &state) {
                report.files += state.len();
                continue;
            }
            verify = true; // 磁盘上少了、或大小不对 → 这一本全量核一遍
        }

        // ② 锁外：探磁盘 → 算计划 → 落盘
        let account = account_of(&state);
        let plan = yanmo_core::store::plan_mirror(
            &desired,
            &state,
            |path| probe(&root, path, &account),
            verify,
        );
        if let Err(error) = execute(&root, &desired, &plan) {
            // **没落成就不记账**：账留在原样，下一轮从原样再来（写是幂等的，重来不亏）
            report.failed_works += 1;
            report.last_error = technical(&error);
            continue;
        }

        // ③ 锁内：记账
        if let Err(error) = data.with_store(|store| store.mirror_record(work, &plan.records)) {
            report.failed_works += 1;
            report.last_error = technical(&error);
            continue;
        }
        report.files += plan.records.len();
        did_work = true;
    }

    // 收尾：把"要作者定夺的事"汇总进报告——**从账上读**，不是从这一轮的计划读。
    // 冲突是一直挂着的，而计划只算"这一轮动过的书"；从账上读，那些被跳过的书
    // （库没变、盘也没变）也照样把冲突报出来，不会因为"这一轮没轮到它"就从界面上消失。
    collect_issues(&data, &root, full || idle, carried_untracked, &mut report);

    report.last_sync_at = yanmo_core::time::now_millis();
    inner.publish(report);
    did_work
}

/// 汇总要作者定夺的事：**账上的冲突**（每轮都数）+ **没账的 `.md`**（只在空闲 / 全量时扫目录）。
///
/// 只读、只报告：**一个文件都不动**。
fn collect_issues(
    data: &AppData,
    root: &std::path::Path,
    scan_files: bool,
    carried_untracked: Vec<MirrorIssue>,
    report: &mut MirrorStatus,
) {
    let rows = data.with_store(|store| store.mirror_conflict_entries()).unwrap_or_default();
    let titles = data
        .with_store(|store| {
            Ok(rows
                .iter()
                .map(|(node_id, _)| store.rendered_title(*node_id).unwrap_or_default())
                .collect::<Vec<String>>())
        })
        .unwrap_or_default();
    let mut issues: Vec<MirrorIssue> = rows
        .iter()
        .zip(titles)
        .map(|((node_id, path), title)| MirrorIssue {
            node_id: *node_id,
            title,
            relative_path: path.clone(),
            // 账上只记"这一份待定夺"，不区分是"被改过"还是"路径上有别人的东西"——
            // 对作者来说都是同一句话：磁盘上这一份与研墨这一边不一样了。
            kind: "edited",
        })
        .collect();
    report.conflicts = issues.len();

    let untracked = if scan_files {
        let known: std::collections::HashSet<String> = data
            .with_store(|store| store.mirror_paths())
            .unwrap_or_default()
            .into_iter()
            .collect();
        let found = crate::mirror_fs::scan_markdown(root);
        let mut total = 0usize;
        let mut extra = Vec::new();
        for path in found {
            if known.contains(&path) {
                continue;
            }
            total += 1;
            if extra.len() < ISSUE_LIST_MAX {
                extra.push(MirrorIssue {
                    node_id: 0,
                    title: String::new(),
                    relative_path: path,
                    kind: "untracked",
                });
            }
        }
        report.untracked = total;
        extra
    } else {
        report.untracked = carried_untracked.len();
        carried_untracked
    };

    issues.extend(untracked.into_iter().take(ISSUE_LIST_MAX.saturating_sub(issues.len())));
    report.issues = issues;
}

/// 账按路径索引（探磁盘时按路径查"这一份记的是什么"）。
fn account_of(state: &[MirrorEntry]) -> HashMap<&str, &MirrorEntry> {
    state.iter().map(|entry| (entry.relative_path.as_str(), entry)).collect()
}



/// 失败的技术说明（进状态给日志味提示，**不是界面文案**）。
fn technical(error: &ApiError) -> String {
    if error.detail.is_empty() {
        error.code.clone()
    } else {
        format!("{} ({})", error.code, error.detail)
    }
}
