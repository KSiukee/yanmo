//! 备份包的**名字口径**与**滚动清理**。
//!
//! 单独成文件（从 [`super::backup`] 里搬出来）：这一块是"备份盘上那些包叫什么名、哪些该轮掉"，
//! 与"怎么写出一个包、清单里记什么、账本怎么写"是两件可以分开读的事——
//! 搬出来之后 `backup.rs` 那边就只剩写包那条链。
//!
//! # 名字是什么口径
//!
//! - 自动备份：`研墨备份-<YYYYMMDD-HHMM>`（加密包再挂一个 `.ympkg`）；
//! - 手动备份：`研墨备份-手动-<YYYYMMDD-HHMM>`——同一个格式，多一个记号。
//!
//! 记号记在**文件名**上，是因为加密包的清单在密文里：名字是唯一**不靠钥匙**就能看的判据。
//! 明文包也按同一个口径认，两条路一条判断，免得两处打架。

use std::path::{Path, PathBuf};

use super::{read_manifest, MANUAL_MARK, PACKAGE_PREFIX};

/// 包名（明文包是目录名，加密包再挂一个 `.ympkg` 后缀）：手动的那一类带上记号。
pub(crate) fn package_name(stamp: &str, manual: bool) -> String {
    if manual {
        format!("{PACKAGE_PREFIX}{MANUAL_MARK}{stamp}")
    } else {
        format!("{PACKAGE_PREFIX}{stamp}")
    }
}

/// 这个名字是不是**手动备份**（`研墨备份-手动-…`）。
///
/// 只看记号、不看有没有时间戳：记号是我们要不要留它的唯一判据，
/// 名字再古怪也不该因此变成"可以删的"。
pub(crate) fn is_manual_package_name(name: &str) -> bool {
    name.starts_with(&format!("{PACKAGE_PREFIX}{MANUAL_MARK}"))
}

/// 从包名里抠出时间戳（`研墨备份-手动-20260930-1300.ympkg` → `20260930-1300`）。
///
/// 恢复列表要对它排序、也要显示"这是哪一刻写的"——**手动记号不是时间的一部分**，得剥掉。
pub(crate) fn stamp_of_package_name(name: &str) -> String {
    let rest = name.strip_prefix(PACKAGE_PREFIX).unwrap_or(name);
    let rest = rest.strip_prefix(MANUAL_MARK).unwrap_or(rest);
    rest.trim_end_matches(&format!(".{}", crate::seal::SEAL_EXT))
        .to_string()
}

/// 从包名里读出**哪一天**（`20260930-1404` → `20260930`；同一分钟的 `-2` 序号不算进日期）。
///
/// 读不出来就返回 `None`——那种名字我们不认，**也不动它**（见 [`plan_prune`]）。
pub fn package_day(name: &str) -> Option<String> {
    let stamp = stamp_of_package_name(name);
    let day = stamp.split('-').next().unwrap_or("");
    let ok = day.len() == 8 && day.bytes().all(|byte| byte.is_ascii_digit());
    ok.then(|| day.to_string())
}

/// 该轮掉哪些包（**纯逻辑**：给一串包名，回一串要删的名字）。
///
/// 规则两条，合起来就是"每天一份、留最近几天"：
///
/// 1. **同一天只留最新的一份**——机器一天可能做好几次（首启、关窗、拔盘前手动触发自动备份），
///    那些彼此之间只差几十分钟，留着只是占盘；一天里最新的那份才是"这一天结束时的样子"。
/// 2. **只留最近 `keep` 天**——按天分组之后，天数就是"能回退多远"，
///    与设置页那句「最近 7 天缺 N 天」是同一个尺度（份数与天数不再是两回事）。
///
/// 三条护栏：
/// - **手动包一个都不删**（名字带记号的一律跳过：那是作者亲手点的）；
/// - **读不出日期的名字不动**（认不出的东西不碰——"清理"最忌讳误删）；
/// - 名字字面量序＝写出序（同一天里最新的是排序最后那个）。
pub fn plan_prune(names: &[String], keep: usize) -> Vec<String> {
    let mut candidates: Vec<&String> = names
        .iter()
        .filter(|name| !is_manual_package_name(name) && package_day(name).is_some())
        .collect();
    candidates.sort(); // 名字里带日期时间，字面量序就是写出序
    let keep = keep.max(1);

    let mut doomed = Vec::new();
    let mut days_kept: Vec<String> = Vec::new();
    // 从最新往回走：这一天还没留过就留下它，留够 keep 天之后的一律轮掉
    for name in candidates.iter().rev() {
        let day = package_day(name).unwrap_or_default();
        if days_kept.contains(&day) {
            doomed.push((*name).clone()); // 这一天已经留了更新的那份
            continue;
        }
        if days_kept.len() < keep {
            days_kept.push(day);
            continue;
        }
        doomed.push((*name).clone());
    }
    doomed.sort(); // 回一个确定的顺序（老的在前面），调用方不必再排
    doomed
}

/// 滚动清理：把备份盘上该轮掉的包删掉，返回删了几份。
///
/// 认得出"这是我们的包"的两条判据：**明文包**是目录（目录名前缀对得上 + 包里有我们的清单）、
/// **加密包**是一个 `.ympkg` 文件（名字前缀 + 扩展名；它的清单在密文里，读不到也不该读）。
/// 不把加密包算进来的话，开了加密之后旧备份就再也不会被轮掉——那是无声的堆积。
///
/// 该轮掉哪些由 [`plan_prune`] 定（每天一份、留最近几天、手动的一个不删）。别人的文件、
/// 半截的目录、读不出日期的名字一律不碰。
pub(crate) fn prune_packages(root: &Path, keep: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut packages: Vec<(String, PathBuf, bool)> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().to_string();
            if !name.starts_with(PACKAGE_PREFIX) {
                return None; // 不是我们的东西
            }
            if path.is_dir() {
                return read_manifest(&path).is_some().then_some((name, path, true));
            }
            crate::store::backup_seal::is_sealed_package(&path).then_some((name, path, false))
        })
        .collect();
    packages.sort_by(|a, b| a.0.cmp(&b.0));

    let names: Vec<String> = packages.iter().map(|(name, _, _)| name.clone()).collect();
    let doomed = plan_prune(&names, keep);

    let mut removed = 0;
    for (name, path, is_dir) in &packages {
        if !doomed.contains(name) {
            continue;
        }
        let done = if *is_dir {
            std::fs::remove_dir_all(path).is_ok()
        } else {
            std::fs::remove_file(path).is_ok()
        };
        if done {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn a_day_reads_off_the_name() {
        assert_eq!(package_day("研墨备份-20260930-1404").as_deref(), Some("20260930"));
        assert_eq!(package_day("研墨备份-20260930-1404-2").as_deref(), Some("20260930"));
        assert_eq!(package_day("研墨备份-手动-20260930-1404.ympkg").as_deref(), Some("20260930"));
        assert_eq!(package_day("研墨备份-没写日期"), None);
        assert_eq!(package_day("研墨备份-2026-09-30"), None);
    }

    #[test]
    fn one_package_per_day_is_enough() {
        // 同一天做了三次：留最新那份，另外两份轮掉
        let doomed = plan_prune(
            &names(&[
                "研墨备份-20260930-0900",
                "研墨备份-20260930-1200",
                "研墨备份-20260930-1800",
            ]),
            7,
        );
        assert_eq!(doomed, names(&["研墨备份-20260930-0900", "研墨备份-20260930-1200"]));
    }

    #[test]
    fn keep_counts_days_not_packages() {
        // 三天各两份、只要最近两天 ⇒ 全留两份（每天一份），其余四份轮掉
        let doomed = plan_prune(
            &names(&[
                "研墨备份-20260928-0900",
                "研墨备份-20260928-2100",
                "研墨备份-20260929-0900",
                "研墨备份-20260929-2100",
                "研墨备份-20260930-0900",
                "研墨备份-20260930-2100",
            ]),
            2,
        );
        assert_eq!(
            doomed,
            names(&[
                "研墨备份-20260928-0900",
                "研墨备份-20260928-2100",
                "研墨备份-20260929-0900",
                "研墨备份-20260930-0900",
            ]),
            "28 号两份全轮掉；29、30 号各留最新那份（2100），各轮掉早上的那份"
        );
    }

    #[test]
    fn manual_packages_are_never_planned_for_deletion() {
        // 手动那份最老、保留份数只有 1：照样不轮它（两套生命周期分开）
        let doomed = plan_prune(
            &names(&[
                "研墨备份-手动-20260101-0900.ympkg",
                "研墨备份-20260929-0900",
                "研墨备份-20260930-0900",
            ]),
            1,
        );
        assert_eq!(doomed, names(&["研墨备份-20260929-0900"]));
    }

    #[test]
    fn names_we_cannot_date_are_left_alone() {
        let doomed = plan_prune(&names(&["研墨备份-手工改过的名字", "研墨备份-20260930-0900"]), 1);
        assert!(doomed.is_empty(), "{doomed:?}");
    }
}
